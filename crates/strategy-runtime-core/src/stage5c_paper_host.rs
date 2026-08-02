use std::collections::{HashMap, HashSet};

use broker_core::{
    BrokerAccountId, BrokerInstrumentSpec, BrokerOrderId, BrokerStopOrderId, InstrumentId,
    RuntimeHostBootstrapSnapshot, Stage4AcceptedPaperHostEvidence,
    Stage4BootstrapEvidenceReportStatus, Stage4BrokerTruthBootstrapStatus,
    Stage4BrokerTruthFreshnessSection, Stage4BrokerTruthFreshnessStatus,
    Stage4BrokerTruthSourceStatus, Stage4DirtyStartPolicyStatus,
    Stage4RuntimeBootstrapApplicationStatus, Stage4RuntimeBootstrapIntegrationEvent,
    Stage4RuntimeBootstrapIntegrationStatus, Stage4RuntimeLifecycleOrderingStatus,
    StrategyRequestId, STAGE4_BOOTSTRAP_EVIDENCE_REPORT_SCHEMA_VERSION,
    STAGE4_RUNTIME_BOOTSTRAP_APPLICATION_SCHEMA_VERSION,
};
use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy;
use crate::runtime_compat::{
    BootstrapSnapshot, GatewayPhase, PaperExecutionMode, PositionEvent, RuntimeStateRestored,
    Strategy, StrategyCtx, StrategyState, TradeMode,
};

pub const STAGE5C_PAPER_HOST_ADMISSION_SCHEMA_VERSION: u16 = 1;
pub const STAGE5C_RUNTIME_STATE_RESTORE_SCHEMA_VERSION: u16 = 1;

// STAGE5D-ADDITIVE-BRIDGE-BEGIN: type-state-transitions
#[cfg(test)]
thread_local! {
    static STAGE5D_RUNTIME_RESTORED_CALLBACK_COUNT: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn stage5d_test_reset_runtime_restored_callback_count() {
    STAGE5D_RUNTIME_RESTORED_CALLBACK_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
pub(crate) fn stage5d_test_runtime_restored_callback_count() -> usize {
    STAGE5D_RUNTIME_RESTORED_CALLBACK_COUNT.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(crate) fn stage5d_test_warmup_stage5c_history_at(
    restored: Stage5cRuntimeStateRestoredPaperStrategy,
    history: Stage5cAcceptedHistoryBatch,
    warmup_now: DateTime<Utc>,
) -> Result<Stage5cWarmedPaperStrategy, Stage5cHistoryWarmupError> {
    warmup_stage5c_history_at(restored, history, warmup_now)
}

#[allow(dead_code)]
pub(crate) fn stage5d_bootstrap_preserving_loaded_with_validated_working_sets_at(
    loaded: Stage5cRuntimeStateLoadedPaperStrategy,
    notification_now: DateTime<Utc>,
    working_orders_strategy: HashMap<BrokerOrderId, crate::runtime_compat::OrderEvent>,
    working_stop_orders_strategy: HashMap<BrokerStopOrderId, crate::runtime_compat::StopOrderEvent>,
) -> Result<
    Stage5cBootstrappedPaperStrategy,
    Box<(
        Stage5cRuntimeStateLoadedPaperStrategy,
        Stage5cBootstrapNotificationError,
    )>,
> {
    let snapshot = loaded.stage5d_admission().bootstrap_snapshot().clone();
    let admission_expires_at = loaded.stage5d_admission().expires_at();
    let admission_account_id = loaded.stage5d_admission().account_id().clone();
    let admission_target_instrument = loaded.stage5d_admission().target_instrument().clone();
    let admission_tick_size = loaded.stage5d_admission().tick_size();
    if notification_now > admission_expires_at {
        return Err(Box::new((
            loaded,
            Stage5cBootstrapNotificationError::AdmissionExpired,
        )));
    }
    let (symbol_matches, tick_size_matches) = loaded
        .stage5d_strategy()
        .stage5c_binding_matches(&admission_target_instrument, admission_tick_size);
    if !symbol_matches {
        return Err(Box::new((
            loaded,
            Stage5cBootstrapNotificationError::StrategyTargetMismatch,
        )));
    }
    if !tick_size_matches {
        return Err(Box::new((
            loaded,
            Stage5cBootstrapNotificationError::StrategyTickSizeMismatch,
        )));
    }
    if snapshot.account_id != admission_account_id {
        return Err(Box::new((
            loaded,
            Stage5cBootstrapNotificationError::SnapshotAccountMismatch,
        )));
    }
    if snapshot.instrument != admission_target_instrument {
        return Err(Box::new((
            loaded,
            Stage5cBootstrapNotificationError::SnapshotInstrumentMismatch,
        )));
    }
    let Some(position_qty) = snapshot
        .target_position_qty
        .to_f64()
        .filter(|value| value.is_finite())
    else {
        return Err(Box::new((
            loaded,
            Stage5cBootstrapNotificationError::PositionQuantityNotRepresentable,
        )));
    };
    let average_price = match snapshot
        .target_open_positions
        .first()
        .and_then(|position| position.avg_price)
        .map(|price| {
            price
                .to_f64()
                .filter(|value| value.is_finite())
                .ok_or(Stage5cBootstrapNotificationError::PositionAveragePriceNotRepresentable)
        })
        .transpose()
    {
        Ok(value) => value.unwrap_or_default(),
        Err(error) => return Err(Box::new((loaded, error))),
    };
    let mut positions_strategy = HashMap::new();
    if !snapshot.target_open_positions.is_empty() || position_qty.abs() > f64::EPSILON {
        positions_strategy.insert(
            snapshot.instrument.symbol.clone(),
            PositionEvent {
                symbol: snapshot.instrument.symbol.clone(),
                qty: position_qty,
                existing: true,
                avg_price: average_price,
                ts_utc: snapshot.received_ts.timestamp(),
            },
        );
    }
    let Stage5cRuntimeStateLoadedPaperStrategy {
        mut strategy,
        admission,
        restored,
        load_origin: _,
    } = loaded;
    let source_snapshot = BootstrapSnapshot {
        positions_strategy,
        working_orders_strategy,
        working_stop_orders_strategy,
        snapshot_ts_utc: Some(snapshot.received_ts.timestamp()),
    };
    let context = StrategyCtx {
        strategy_id: admission.strategy_id().to_string(),
        portfolio: admission.account_id().as_str().to_string(),
        exchange: format!("{:?}", admission.target_instrument().exchange),
        symbol: admission.target_instrument().symbol.clone(),
        tick_size: admission.tick_size(),
        trade_mode: TradeMode::Paper,
        paper_execution_mode: PaperExecutionMode::LiveOnly,
        allow_live_orders: false,
        gateway_phase: GatewayPhase::SyncingHistory,
        position_qty: Some(position_qty),
        event_ts_utc: snapshot.received_ts.timestamp(),
        now_ts_utc: notification_now.timestamp(),
        last_bar_ts: None,
    };
    let intents = Strategy::on_bootstrap_snapshot(&mut strategy, &context, &source_snapshot);
    debug_assert!(
        intents.is_empty(),
        "accepted Stage 5D working-order bootstrap callback must not emit intents"
    );
    Ok(Stage5cBootstrappedPaperStrategy {
        strategy,
        receipt: Stage5cBootstrapNotificationReceipt {
            admission,
            notified_ts: notification_now,
        },
        restored,
    })
}

impl Stage5cRuntimeStateLoadedPaperStrategy {
    pub(crate) fn stage5d_strategy(&self) -> &HybridIntradayRuntimeStrategy {
        &self.strategy
    }

    pub(crate) fn stage5d_admission(&self) -> &Stage5cPaperHostAdmission {
        &self.admission
    }

    pub(crate) fn stage5d_restored(&self) -> &RuntimeStateRestored {
        &self.restored
    }

    pub(crate) fn stage5d_load_origin(&self) -> &Stage5cRuntimeStateLoadOrigin {
        &self.load_origin
    }

    pub(crate) fn stage5d_into_parts(
        self,
    ) -> (
        HybridIntradayRuntimeStrategy,
        Stage5cPaperHostAdmission,
        RuntimeStateRestored,
        Stage5cRuntimeStateLoadOrigin,
    ) {
        let Self {
            strategy,
            admission,
            restored,
            load_origin,
        } = self;
        (strategy, admission, restored, load_origin)
    }

    pub(crate) fn stage5d_from_parts(
        strategy: HybridIntradayRuntimeStrategy,
        admission: Stage5cPaperHostAdmission,
        restored: RuntimeStateRestored,
        load_origin: Stage5cRuntimeStateLoadOrigin,
    ) -> Self {
        Self {
            strategy,
            admission,
            restored,
            load_origin,
        }
    }

    #[cfg(test)]
    pub(crate) fn stage5d_test_loaded_from_parts(
        strategy: HybridIntradayRuntimeStrategy,
        admission: Stage5cPaperHostAdmission,
        restored: RuntimeStateRestored,
        load_origin: Stage5cRuntimeStateLoadOrigin,
    ) -> Self {
        Self {
            strategy,
            admission,
            restored,
            load_origin,
        }
    }
}

impl Stage5cRuntimeStateRestoredPaperStrategy {
    #[cfg(test)]
    pub(crate) fn stage5d_strategy(&self) -> &HybridIntradayRuntimeStrategy {
        &self.strategy
    }

    #[cfg(test)]
    pub(crate) fn stage5d_test_restored_from_parts(
        strategy: HybridIntradayRuntimeStrategy,
        receipt: Stage5cRuntimeStateRestoreReceipt,
    ) -> Self {
        Self { strategy, receipt }
    }
}

impl Stage5cBootstrappedPaperStrategy {
    pub(crate) fn stage5d_strategy(&self) -> &HybridIntradayRuntimeStrategy {
        &self.strategy
    }

    pub(crate) fn stage5d_admission(&self) -> &Stage5cPaperHostAdmission {
        &self.receipt.admission
    }

    pub(crate) fn stage5d_restored(&self) -> &RuntimeStateRestored {
        &self.restored
    }

    pub(crate) fn stage5d_bootstrap_notified_ts(&self) -> DateTime<Utc> {
        self.receipt.notified_ts
    }

    #[cfg(test)]
    pub(crate) fn stage5d_test_set_strategy_state(&mut self, state: StrategyState) {
        Strategy::set_state(&mut self.strategy, state);
    }

    #[cfg(test)]
    pub(crate) fn stage5d_test_mark_runtime_host_attached(&mut self) {
        self.receipt.admission.runtime_host_attached = true;
    }

    #[cfg(test)]
    pub(crate) fn stage5d_test_mark_intent_sink_attached(&mut self) {
        self.receipt.admission.intent_sink_attached = true;
    }

    #[cfg(test)]
    pub(crate) fn stage5d_test_mark_not_paper_only(&mut self) {
        self.receipt.admission.paper_only = false;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage5dRuntimeStateRestoredBridgeError {
    Stage5c(Stage5cRuntimeStateRestoreError),
    CallbackEmittedIntent,
}

pub(crate) struct Stage5dBrokerOwnedIdNormalizationBlocked {
    pub(crate) bootstrapped: Box<Stage5cBootstrappedPaperStrategy>,
    pub(crate) reason: Stage5dRuntimeStateRestoredBridgeError,
}

pub(crate) fn stage5d_notify_runtime_state_restored_bridge_at(
    bootstrapped: Stage5cBootstrappedPaperStrategy,
    restored_ts: DateTime<Utc>,
) -> Result<Stage5cRuntimeStateRestoredPaperStrategy, Stage5dRuntimeStateRestoredBridgeError> {
    stage5d_notify_runtime_state_restored_bridge_impl_at(
        bootstrapped,
        restored_ts,
        Stage5dRuntimeRestoredBridgeTestHook::None,
    )
}

pub(crate) fn stage5d_normalize_broker_owned_ids_for_closed_restore_bridge(
    bootstrapped: Stage5cBootstrappedPaperStrategy,
    expected_working_order_ids: &[BrokerOrderId],
    expected_working_stop_order_ids: &[BrokerStopOrderId],
) -> Result<Stage5cBootstrappedPaperStrategy, Stage5dBrokerOwnedIdNormalizationBlocked> {
    let (mut strategy, receipt, restored) = bootstrapped.into_parts();
    let mut state = Strategy::state(&strategy).clone();
    let StrategyState::HybridIntradayRuntime {
        tp_order_id,
        sl_stop_order_id,
        sl_exchange_order_id,
        ..
    } = &mut state
    else {
        return Err(Stage5dBrokerOwnedIdNormalizationBlocked {
            bootstrapped: Box::new(Stage5cBootstrappedPaperStrategy {
                strategy,
                receipt,
                restored,
            }),
            reason: Stage5dRuntimeStateRestoredBridgeError::Stage5c(
                Stage5cRuntimeStateRestoreError::WrongStrategyStateKind,
            ),
        });
    };
    let tp_is_frozen = tp_order_id
        .as_ref()
        .map_or(true, |id| expected_working_order_ids.contains(id));
    let sl_stop_is_frozen = sl_stop_order_id
        .as_ref()
        .map_or(true, |id| expected_working_stop_order_ids.contains(id));
    let sl_exchange_is_frozen = sl_exchange_order_id
        .as_ref()
        .map_or(true, |id| expected_working_order_ids.contains(id));
    if !tp_is_frozen || !sl_stop_is_frozen || !sl_exchange_is_frozen {
        return Err(Stage5dBrokerOwnedIdNormalizationBlocked {
            bootstrapped: Box::new(Stage5cBootstrappedPaperStrategy {
                strategy,
                receipt,
                restored,
            }),
            reason: Stage5dRuntimeStateRestoredBridgeError::Stage5c(
                Stage5cRuntimeStateRestoreError::BrokerOwnedOrderIdMismatch,
            ),
        });
    }
    *tp_order_id = None;
    *sl_stop_order_id = None;
    *sl_exchange_order_id = None;
    Strategy::set_state(&mut strategy, state);
    Ok(Stage5cBootstrappedPaperStrategy {
        strategy,
        receipt,
        restored,
    })
}

#[cfg(test)]
pub(crate) fn stage5d_test_notify_runtime_state_restored_bridge_forcing_intent_at(
    bootstrapped: Stage5cBootstrappedPaperStrategy,
    restored_ts: DateTime<Utc>,
) -> Result<Stage5cRuntimeStateRestoredPaperStrategy, Stage5dRuntimeStateRestoredBridgeError> {
    stage5d_notify_runtime_state_restored_bridge_impl_at(
        bootstrapped,
        restored_ts,
        Stage5dRuntimeRestoredBridgeTestHook::ForceIntent,
    )
}

#[cfg(test)]
pub(crate) fn stage5d_test_notify_runtime_state_restored_bridge_with_state_override_at(
    bootstrapped: Stage5cBootstrappedPaperStrategy,
    restored_ts: DateTime<Utc>,
    state: StrategyState,
) -> Result<Stage5cRuntimeStateRestoredPaperStrategy, Stage5dRuntimeStateRestoredBridgeError> {
    stage5d_notify_runtime_state_restored_bridge_impl_at(
        bootstrapped,
        restored_ts,
        Stage5dRuntimeRestoredBridgeTestHook::OverrideStateAfterCallback(Box::new(state)),
    )
}

enum Stage5dRuntimeRestoredBridgeTestHook {
    None,
    #[cfg(test)]
    ForceIntent,
    #[cfg(test)]
    OverrideStateAfterCallback(Box<StrategyState>),
}

fn stage5d_notify_runtime_state_restored_bridge_impl_at(
    bootstrapped: Stage5cBootstrappedPaperStrategy,
    restored_ts: DateTime<Utc>,
    _test_hook: Stage5dRuntimeRestoredBridgeTestHook,
) -> Result<Stage5cRuntimeStateRestoredPaperStrategy, Stage5dRuntimeStateRestoredBridgeError> {
    let (mut strategy, bootstrap_receipt, restored) = bootstrapped.into_parts();
    let admission = &bootstrap_receipt.admission;
    let broker_position_qty = admission
        .bootstrap_snapshot()
        .target_position_qty
        .to_f64()
        .ok_or(Stage5dRuntimeStateRestoredBridgeError::Stage5c(
            Stage5cRuntimeStateRestoreError::BrokerTruthPositionMismatch,
        ))?;
    let context = StrategyCtx {
        strategy_id: admission.strategy_id().to_string(),
        portfolio: admission.account_id().as_str().to_string(),
        exchange: format!("{:?}", admission.target_instrument().exchange),
        symbol: admission.target_instrument().symbol.clone(),
        tick_size: admission.tick_size(),
        trade_mode: TradeMode::Paper,
        paper_execution_mode: PaperExecutionMode::LiveOnly,
        allow_live_orders: false,
        gateway_phase: GatewayPhase::SyncingHistory,
        position_qty: Some(broker_position_qty),
        event_ts_utc: restored_ts.timestamp(),
        now_ts_utc: restored_ts.timestamp(),
        last_bar_ts: None,
    };
    let known_order_ids = restored.known_order_ids.clone();
    let pending_requests = restored.pending_requests.clone();
    #[cfg(test)]
    STAGE5D_RUNTIME_RESTORED_CALLBACK_COUNT.with(|count| count.set(count.get() + 1));
    let intents = Strategy::on_runtime_state_restored(&mut strategy, &context, &restored);
    #[cfg(test)]
    let mut intents = intents;
    #[cfg(test)]
    if matches!(
        _test_hook,
        Stage5dRuntimeRestoredBridgeTestHook::ForceIntent
    ) {
        intents.push(crate::runtime_compat::Intent::Market {
            qty: 0.0,
            side: crate::runtime_compat::OrderSide::Buy,
            fill_price: None,
            comment: Some("stage5d-test-synthetic-restored-intent".to_string()),
        });
    }
    if !intents.is_empty() {
        return Err(Stage5dRuntimeStateRestoredBridgeError::CallbackEmittedIntent);
    }
    debug_assert!(intents.is_empty());
    #[cfg(test)]
    if let Stage5dRuntimeRestoredBridgeTestHook::OverrideStateAfterCallback(state) = _test_hook {
        Strategy::set_state(&mut strategy, *state);
    }
    validate_post_bootstrap_broker_truth(&strategy, admission)
        .map_err(Stage5dRuntimeStateRestoredBridgeError::Stage5c)?;
    stage5d_validate_post_runtime_restored_broker_truth_exact(&strategy, admission)?;

    Ok(Stage5cRuntimeStateRestoredPaperStrategy {
        strategy,
        receipt: Stage5cRuntimeStateRestoreReceipt {
            bootstrap_receipt,
            restored_ts,
            known_order_ids,
            pending_requests,
        },
    })
}

fn stage5d_validate_post_runtime_restored_broker_truth_exact(
    strategy: &HybridIntradayRuntimeStrategy,
    admission: &Stage5cPaperHostAdmission,
) -> Result<(), Stage5dRuntimeStateRestoredBridgeError> {
    let state = Strategy::state(strategy);
    let broker_qty = admission
        .bootstrap_snapshot()
        .target_position_qty
        .to_f64()
        .filter(|value| value.is_finite())
        .ok_or(Stage5dRuntimeStateRestoredBridgeError::Stage5c(
            Stage5cRuntimeStateRestoreError::BrokerTruthPositionMismatch,
        ))?;
    let StrategyState::HybridIntradayRuntime {
        last_position_qty,
        current_side,
        tp_order_id,
        sl_stop_order_id,
        sl_exchange_order_id,
        ..
    } = state
    else {
        return Err(Stage5dRuntimeStateRestoredBridgeError::Stage5c(
            Stage5cRuntimeStateRestoreError::WrongStrategyStateKind,
        ));
    };
    if (*last_position_qty - broker_qty).abs() > f64::EPSILON {
        return Err(Stage5dRuntimeStateRestoredBridgeError::Stage5c(
            Stage5cRuntimeStateRestoreError::BrokerTruthPositionMismatch,
        ));
    }
    let expected_side = if broker_qty > f64::EPSILON {
        Some(crate::hybrid_intraday::Side::Long)
    } else if broker_qty < -f64::EPSILON {
        Some(crate::hybrid_intraday::Side::Short)
    } else {
        None
    };
    if *current_side != expected_side {
        return Err(Stage5dRuntimeStateRestoredBridgeError::Stage5c(
            Stage5cRuntimeStateRestoreError::BrokerTruthSideMismatch,
        ));
    }
    if tp_order_id.is_some() || sl_stop_order_id.is_some() || sl_exchange_order_id.is_some() {
        return Err(Stage5dRuntimeStateRestoredBridgeError::Stage5c(
            Stage5cRuntimeStateRestoreError::BrokerOwnedOrderIdMismatch,
        ));
    }
    Ok(())
}

#[allow(dead_code)]
pub(crate) fn stage5d_bootstrap_preserving_loaded_at(
    loaded: Stage5cRuntimeStateLoadedPaperStrategy,
    notification_now: DateTime<Utc>,
) -> Result<
    Stage5cBootstrappedPaperStrategy,
    Box<(
        Stage5cRuntimeStateLoadedPaperStrategy,
        Stage5cBootstrapNotificationError,
    )>,
> {
    if let Err(error) = validate_stage5cb_notification(
        loaded.stage5d_strategy(),
        loaded.stage5d_admission(),
        notification_now,
    ) {
        return Err(Box::new((loaded, error)));
    }
    let snapshot = loaded.stage5d_admission().bootstrap_snapshot();
    if snapshot
        .target_position_qty
        .to_f64()
        .filter(|value| value.is_finite())
        .is_none()
    {
        return Err(Box::new((
            loaded,
            Stage5cBootstrapNotificationError::PositionQuantityNotRepresentable,
        )));
    }
    if snapshot
        .target_open_positions
        .first()
        .and_then(|position| position.avg_price)
        .is_some_and(|price| price.to_f64().filter(|value| value.is_finite()).is_none())
    {
        return Err(Box::new((
            loaded,
            Stage5cBootstrapNotificationError::PositionAveragePriceNotRepresentable,
        )));
    }

    Ok(notify_stage5c_bootstrap_at(loaded, notification_now)
        .expect("Stage 5D prevalidated bootstrap notification must not fail"))
}

pub(crate) fn stage5d_inject_authoritative_riskgate_state(
    bootstrapped: Stage5cBootstrappedPaperStrategy,
    riskgate: crate::runtime_compat::RiskGateRuntimeState,
) -> Stage5cBootstrappedPaperStrategy {
    let (mut strategy, receipt, restored) = bootstrapped.into_parts();
    Strategy::on_risk_gate_state(&mut strategy, &riskgate);
    Stage5cBootstrappedPaperStrategy {
        strategy,
        receipt,
        restored,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Stage5cRuntimeStateLoadOrigin {
    CleanStart,
    Persisted {
        semantic_payload_fingerprint: String,
        persisted_ts: DateTime<Utc>,
        recovery_index_fingerprint: String,
    },
}

#[cfg(test)]
impl Stage5cPaperHostAdmission {
    pub(crate) fn stage5d_test_new(
        strategy_id: String,
        account_id: BrokerAccountId,
        target_instrument: InstrumentId,
        tick_size: f64,
        target_position_qty: rust_decimal::Decimal,
        checked_ts: DateTime<Utc>,
    ) -> Self {
        let bootstrap_snapshot = RuntimeHostBootstrapSnapshot {
            account_id: account_id.clone(),
            instrument: target_instrument.clone(),
            target_position_qty,
            target_open_positions: Vec::new(),
            target_active_orders: Vec::new(),
            account_active_orders_count: 0,
            target_is_flat: target_position_qty == rust_decimal::Decimal::ZERO,
            received_ts: checked_ts,
        };
        Self {
            schema_version: STAGE5C_PAPER_HOST_ADMISSION_SCHEMA_VERSION,
            checked_ts,
            issued_ts: checked_ts,
            expires_at: checked_ts + chrono::Duration::hours(1),
            strategy_id,
            account_id,
            target_instrument,
            tick_size,
            bootstrap_snapshot,
            paper_only: true,
            runtime_host_attached: false,
            intent_sink_attached: false,
        }
    }

    pub(crate) fn stage5d_test_with_target_active_orders(
        mut self,
        target_active_orders: Vec<broker_core::BrokerOrderSnapshot>,
    ) -> Self {
        self.bootstrap_snapshot.account_active_orders_count = target_active_orders.len();
        self.bootstrap_snapshot.target_active_orders = target_active_orders;
        self
    }

    pub(crate) fn stage5d_test_with_target_position_qty(
        mut self,
        target_position_qty: rust_decimal::Decimal,
    ) -> Self {
        self.bootstrap_snapshot.target_position_qty = target_position_qty;
        self.bootstrap_snapshot.target_is_flat = target_position_qty == rust_decimal::Decimal::ZERO;
        self
    }

    pub(crate) fn stage5d_test_with_target_open_positions(
        mut self,
        target_open_positions: Vec<broker_core::BrokerPositionSnapshot>,
    ) -> Self {
        self.bootstrap_snapshot.target_open_positions = target_open_positions;
        self
    }
}

#[cfg(test)]
mod stage5d_pair_binding_restore_tests {
    use super::*;
    use rust_decimal::Decimal;
    use serde_json::Value;

    fn stage5d_test_strategy() -> HybridIntradayRuntimeStrategy {
        HybridIntradayRuntimeStrategy::new(
            crate::hybrid_intraday_runtime::HybridIntradayRuntimeConfig {
                symbol: "IMOEXF".to_string(),
                profile:
                    crate::hybrid_intraday_runtime::HybridIntradayProfile::BaselineRuntimeHybrid,
                mr_variant:
                    crate::hybrid_intraday_runtime::MeanReversionVariant::ClassicPrevDayRange,
                mr_gate_policy: crate::hybrid_intraday_runtime::MrGatePolicy::Disabled,
                risk_gate_mode: crate::hybrid_intraday_runtime::RiskGateMode::Disabled,
                risk_gate_seed_file: None,
                risk_gate_ledger_key: None,
                model_session_start_time: None,
                model_session_end_time: None,
                qty: 1.0,
                live_order_style: crate::runtime_compat::MarketBuyAndCloseLiveOrderStyle::Market,
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
                pending_timeout_sec: 30,
                partial_entry_fill_timeout_ms: 3_000,
                mr_config: crate::hybrid_intraday::MeanReversionConfig::default(),
                breakout_config: crate::hybrid_intraday::IntradayBreakoutConfig::default(),
                orchestrator_config: crate::hybrid_intraday::HybridOrchestratorConfig::default(),
            },
        )
    }

    fn stage5d_apply_valid_fixture() -> crate::stage5d_persistence::Stage5dPersistenceEnvelope {
        let mut envelope: crate::stage5d_persistence::Stage5dPersistenceEnvelope =
            serde_json::from_str(include_str!(
                "../../../tests/fixtures/stage5/stage5d_b2a_persistence_envelope.json"
            ))
            .expect("fixture");
        let entry = envelope
            .runtime_private_extension
            .pending_entry
            .as_mut()
            .expect("pending entry");
        entry.owner = crate::stage5d_persistence::Stage5dOwner::MeanReversion;
        entry.side = crate::stage5d_persistence::Stage5dSide::Long;
        entry.entry_style = crate::stage5d_persistence::Stage5dEntryStyle::Bracket;
        entry.target_qty = "3".to_string();
        envelope
    }

    fn bind_fixture_to_strategy_config(
        envelope: &mut crate::stage5d_persistence::Stage5dPersistenceEnvelope,
        strategy: &HybridIntradayRuntimeStrategy,
    ) {
        let (profile, mr_variant, mr_gate_policy, risk_gate_mode) =
            strategy.stage5c_profile_binding();
        let profile_binding = format!("{profile}|{mr_variant}|{mr_gate_policy}|{risk_gate_mode}");
        let canonical = strategy.stage5d_canonical_config_fingerprint();
        envelope.binding.stage5c_compat_config_fingerprint = strategy.stage5c_config_fingerprint();
        envelope.binding.profile_binding = profile_binding;
        envelope.binding.stage5d_canonical_config_fingerprint = canonical.clone();
        envelope.canonical_config_fingerprint = canonical;
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum");
    }

    fn stage5c_restore_input_for(
        strategy: &HybridIntradayRuntimeStrategy,
        envelope: &crate::stage5d_persistence::Stage5dPersistenceEnvelope,
    ) -> Stage5cRuntimeStateRestoreInput {
        let (profile, mr_variant, mr_gate_policy, risk_gate_mode) =
            strategy.stage5c_profile_binding();
        Stage5cRuntimeStateRestoreInput {
            schema_version: STAGE5C_RUNTIME_STATE_RESTORE_SCHEMA_VERSION,
            state_schema_version: 1,
            strategy_kind: "hybrid_intraday_runtime".to_string(),
            strategy_id: envelope.binding.strategy_id.clone(),
            account_id: envelope.binding.account_id.clone(),
            instrument: envelope.binding.instrument_id.to_instrument_id(),
            tick_size: 0.5,
            config_fingerprint: strategy.stage5c_config_fingerprint(),
            profile,
            mr_variant,
            mr_gate_policy,
            risk_gate_mode,
            persisted_ts: envelope.persisted_at_ts_utc,
            state_json: serde_json::to_string(&envelope.strategy_state.strategy_state_json)
                .expect("state json"),
            known_order_ids: envelope.recovery_indexes.known_order_ids.clone(),
            pending_requests: envelope.recovery_indexes.pending_requests.clone(),
            legacy_numeric_order_id_policy: Stage5cLegacyNumericOrderIdPolicy::Reject,
        }
    }

    fn expect_stage5d_bind_ok<T>(
        result: Result<T, crate::stage5d_persistence::Stage5dRuntimePrivateApplyBlocked>,
        message: &str,
    ) -> T {
        match result {
            Ok(value) => value,
            Err(blocked) => panic!("{message}: {:?}", blocked.reason()),
        }
    }

    #[test]
    fn stage5d_b2b_real_stage5c_restore_entry_ready_normalization_still_binds() {
        let mut envelope = stage5d_apply_valid_fixture();
        let strategy = stage5d_test_strategy();
        if let Value::Object(fields) =
            &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
        {
            fields.insert("entry_ready".to_string(), Value::Bool(true));
            fields.insert("last_bar_close".to_string(), Value::Null);
            fields.insert("prev_day_close".to_string(), Value::Null);
            fields.insert("current_day_high".to_string(), Value::Null);
            fields.insert("current_day_low".to_string(), Value::Null);
            fields.insert("prev_day_range".to_string(), Value::Null);
            fields.insert("current_day_close".to_string(), Value::Null);
            fields.insert("prev_day_return".to_string(), Value::Null);
            fields.insert("day_before_close".to_string(), Value::Null);
            fields.insert("today_start_local".to_string(), Value::Null);
        }
        bind_fixture_to_strategy_config(&mut envelope, &strategy);
        let admission = Stage5cPaperHostAdmission::stage5d_test_new(
            envelope.binding.strategy_id.clone(),
            envelope.binding.account_id.clone(),
            envelope.binding.instrument_id.to_instrument_id(),
            0.5,
            Decimal::new(5, 1),
            Utc::now(),
        );
        let input = stage5c_restore_input_for(&strategy, &envelope);
        let loaded = restore_stage5c_runtime_state(strategy, admission, input)
            .expect("real Stage 5C restore must load persisted state");
        let current = serde_json::to_value(Strategy::state(loaded.stage5d_strategy()))
            .expect("current state");
        assert_eq!(
            current["HybridIntradayRuntime"]["entry_ready"],
            Value::Bool(false)
        );

        let validated = envelope
            .validate_restore_contract_schema_only()
            .expect("entry_ready=true envelope remains schema valid");
        let bound = expect_stage5d_bind_ok(
            crate::stage5d_persistence::stage5d_bind_runtime_state_loaded(loaded, validated),
            "persisted-owned projection must ignore pre-warmup entry_ready normalization",
        );
        assert_eq!(bound.snapshot_id(), "SNAP_STAGE5D_B2A_0001");
    }
}

// STAGE5E-NO-IO-BRIDGE-BEGIN: contextual-observation-v1
/// Private construction seal: only this Stage 5C bridge can build Stage 5E inputs.
/// It is intentionally neither serializable nor publicly constructible.
pub(crate) struct Stage5eNoIoBridgeSeal(());

// STAGE5E-B3C-R6-SEALS-BEGIN: additive-no-io-v1
// These seals carry no callback, intent, transport or broker capability.
// Candidate construction happens before schedule classification; the final
// identity and returned projection exist only in the classified seal.
#[allow(dead_code)] // Runtime orchestration remains closed in this stage.
pub(crate) struct Stage5cSequenceCandidateSeal {
    instrument: InstrumentId,
    predecessor_close_ts: i64,
    current_close_ts: i64,
    timeframe_sec: std::num::NonZeroU32,
    stage3_provenance_identity: [u8; 32],
    semantic_bar_identity: [u8; 32],
    recovery_receipt_identity: [u8; 32],
    sequence_observed_at: DateTime<Utc>,
    sequence_expires_at: DateTime<Utc>,
}

pub(crate) struct Stage5cClassifiedSequenceSeal {
    candidate: Stage5cSequenceCandidateSeal,
    classification:
        crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eScheduleSequenceClassification,
    boundary_fingerprint: Option<[u8; 32]>,
    sequence_identity_fingerprint: [u8; 32],
    returned_projection:
        crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eScheduleProjectionBridgeInput,
}

#[allow(dead_code)] // Exercised by the sealed no-I/O implementation tests.
impl Stage5cSequenceCandidateSeal {
    fn classify_with_owned_projection(
        self,
        classifier: crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eScheduleCandidateClassifier,
    ) -> Result<
        Stage5cClassifiedSequenceSeal,
        Box<
            crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eScheduleClassificationBlocked,
        >,
    >{
        let approved = classifier.classify_from_stage5c_seal_fields(
            self.predecessor_close_ts,
            self.current_close_ts,
            self.timeframe_sec,
        )?;
        let (classification, returned_projection) = approved.into_classified_parts();
        let (classification_code, boundary_fingerprint) = match classification {
            crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eScheduleSequenceClassification::Contiguous => {
                (1, None)
            }
            crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eScheduleSequenceClassification::ApprovedNonTradableBoundary(value) => {
                (2, Some(value))
            }
        };
        let sequence_identity_fingerprint = stage5e_b3c_final_sequence_identity(
            self.stage3_provenance_identity,
            self.semantic_bar_identity,
            self.recovery_receipt_identity,
            self.predecessor_close_ts,
            self.timeframe_sec,
            self.sequence_observed_at,
            self.sequence_expires_at,
            classification_code,
            boundary_fingerprint,
        );
        Ok(Stage5cClassifiedSequenceSeal {
            candidate: self,
            classification,
            boundary_fingerprint,
            sequence_identity_fingerprint,
            returned_projection,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn stage5e_b3c_final_sequence_identity(
    stage3_provenance_identity: [u8; 32],
    semantic_bar_identity: [u8; 32],
    recovery_receipt_identity: [u8; 32],
    predecessor_close_ts: i64,
    timeframe_sec: std::num::NonZeroU32,
    sequence_observed_at: DateTime<Utc>,
    sequence_expires_at: DateTime<Utc>,
    classification_code: u8,
    boundary_fingerprint: Option<[u8; 32]>,
) -> [u8; 32] {
    let mut encoder = Stage5eB3cCanonicalEncoder::new(b"stage5e-b3c-market-sequence-v2");
    encoder.field(1, &stage3_provenance_identity);
    encoder.field(2, &semantic_bar_identity);
    encoder.field(3, &recovery_receipt_identity);
    encoder.field(4, &predecessor_close_ts.to_be_bytes());
    encoder.field(5, &timeframe_sec.get().to_be_bytes());
    encoder.field(6, &sequence_observed_at.timestamp_millis().to_be_bytes());
    encoder.field(7, &sequence_expires_at.timestamp_millis().to_be_bytes());
    encoder.field(8, &[classification_code]);
    match boundary_fingerprint {
        Some(value) => {
            encoder.field(9, &[1]);
            encoder.field(10, &value);
        }
        None => encoder.field(9, &[0]),
    }
    encoder.finish()
}

#[allow(dead_code, clippy::too_many_arguments)]
fn build_stage5c_sequence_candidate_seal_inside_stage5e_try_observe_live_bar_after_history_with_sequence_evidence(
    instrument: InstrumentId,
    predecessor_close_ts: i64,
    current_close_ts: i64,
    timeframe_sec: std::num::NonZeroU32,
    stage3_provenance_identity: [u8; 32],
    semantic_bar_identity: [u8; 32],
    recovery_receipt_identity: [u8; 32],
    sequence_observed_at: DateTime<Utc>,
    sequence_expires_at: DateTime<Utc>,
) -> Option<Stage5cSequenceCandidateSeal> {
    (sequence_observed_at <= sequence_expires_at).then_some(Stage5cSequenceCandidateSeal {
        instrument,
        predecessor_close_ts,
        current_close_ts,
        timeframe_sec,
        stage3_provenance_identity,
        semantic_bar_identity,
        recovery_receipt_identity,
        sequence_observed_at,
        sequence_expires_at,
    })
}

struct Stage5eB3cCanonicalEncoder {
    hasher: Sha256,
}

impl Stage5eB3cCanonicalEncoder {
    fn new(domain: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(domain);
        Self { hasher }
    }

    fn field(&mut self, tag: u8, bytes: &[u8]) {
        self.hasher.update([tag]);
        self.hasher.update((bytes.len() as u64).to_be_bytes());
        self.hasher.update(bytes);
    }

    fn finish(self) -> [u8; 32] {
        self.hasher.finalize().into()
    }
}

fn stage5e_b3c_string_field(encoder: &mut Stage5eB3cCanonicalEncoder, tag: u8, value: &str) {
    encoder.field(tag, value.as_bytes());
}

fn stage5e_b3c_encode_instrument(
    encoder: &mut Stage5eB3cCanonicalEncoder,
    instrument: &InstrumentId,
) {
    stage5e_b3c_string_field(encoder, 1, &instrument.symbol);
    match instrument.venue_symbol.as_deref() {
        Some(value) => {
            encoder.field(2, &[1]);
            stage5e_b3c_string_field(encoder, 3, value);
        }
        None => encoder.field(2, &[0]),
    }
    match &instrument.exchange {
        broker_core::Exchange::Moex => encoder.field(4, &[1]),
        broker_core::Exchange::Other(value) => {
            encoder.field(4, &[255]);
            stage5e_b3c_string_field(encoder, 5, value);
        }
    }
    match &instrument.market {
        broker_core::Market::Futures => encoder.field(6, &[1]),
        broker_core::Market::Options => encoder.field(6, &[2]),
        broker_core::Market::Stocks => encoder.field(6, &[3]),
        broker_core::Market::Currency => encoder.field(6, &[4]),
        broker_core::Market::Funds => encoder.field(6, &[5]),
        broker_core::Market::Other(value) => {
            encoder.field(6, &[255]);
            stage5e_b3c_string_field(encoder, 7, value);
        }
    }
}

fn stage5e_b3c_stage3_source_mode_code(
    source_mode: broker_core::Stage3StrategyBarSourceMode,
) -> u8 {
    match source_mode {
        broker_core::Stage3StrategyBarSourceMode::AlorNativeBarsGetAndSubscribeTf600 => 1,
        broker_core::Stage3StrategyBarSourceMode::AlorStandDerivedM1ToM10 => 2,
        broker_core::Stage3StrategyBarSourceMode::FinamDerivedM1ToM10 => 3,
        broker_core::Stage3StrategyBarSourceMode::FinamNativeM10 => 4,
        broker_core::Stage3StrategyBarSourceMode::RawFinamM1 => 5,
    }
}

fn stage5e_b3c_stage3_provenance_identity(
    provenance: &broker_core::Stage3StrategyBarProvenance,
) -> [u8; 32] {
    let mut encoder = Stage5eB3cCanonicalEncoder::new(b"stage5e-b3c-stage3-provenance-v1");
    encoder.field(
        1,
        &[stage5e_b3c_stage3_source_mode_code(provenance.source_mode)],
    );
    match provenance.source_timeframe_sec {
        Some(value) => {
            encoder.field(2, &[1]);
            encoder.field(3, &value.to_be_bytes());
        }
        None => encoder.field(2, &[0]),
    }
    encoder.field(4, &provenance.target_timeframe_sec.to_be_bytes());
    encoder.field(5, &[u8::from(provenance.aggregation_complete)]);
    encoder.field(6, &[u8::from(provenance.gap_absence_proven)]);
    encoder.finish()
}

fn stage5e_b3c_semantic_bar_identity(
    bar: &broker_core::HybridRuntimeBarEvent,
    stage3_provenance_identity: [u8; 32],
) -> [u8; 32] {
    let mut encoder = Stage5eB3cCanonicalEncoder::new(b"stage5e-b3c-semantic-bar-v1");
    stage5e_b3c_encode_instrument(&mut encoder, &bar.instrument);
    encoder.field(10, &bar.close_time_utc.to_be_bytes());
    encoder.field(11, &bar.open.to_bits().to_be_bytes());
    encoder.field(12, &bar.high.to_bits().to_be_bytes());
    encoder.field(13, &bar.low.to_bits().to_be_bytes());
    encoder.field(14, &bar.close.to_bits().to_be_bytes());
    encoder.field(15, &bar.volume.to_bits().to_be_bytes());
    let origin_code = match bar.origin {
        broker_core::HybridRuntimeBarOrigin::History => 1,
        broker_core::HybridRuntimeBarOrigin::HistoryGap => 2,
        broker_core::HybridRuntimeBarOrigin::Live => 3,
        broker_core::HybridRuntimeBarOrigin::Replay => 4,
    };
    encoder.field(16, &[origin_code]);
    encoder.field(17, &[u8::from(bar.is_final)]);
    encoder.field(18, &bar.timeframe_sec.to_be_bytes());
    encoder.field(19, &stage3_provenance_identity);
    encoder.finish()
}

#[allow(dead_code)] // Used only by the currently closed sequence issuer.
fn stage5e_b3c_recovery_receipt_identity(receipt: &Stage5cPendingRecoveryReceipt) -> [u8; 32] {
    let warmup = receipt.warmup_receipt();
    let restore = warmup.restore_receipt();
    let mut encoder = Stage5eB3cCanonicalEncoder::new(b"stage5e-b3c-recovery-receipt-v1");
    encoder.field(1, &restore.restored_ts().timestamp_millis().to_be_bytes());
    encoder.field(2, &warmup.started_ts().timestamp_millis().to_be_bytes());
    encoder.field(3, &(warmup.processed_bars() as u64).to_be_bytes());
    encoder.field(4, &(warmup.input_bars() as u64).to_be_bytes());
    encoder.field(
        5,
        &[stage5e_b3c_stage3_source_mode_code(warmup.source_mode())],
    );
    encoder.field(6, &warmup.last_history_ts().to_be_bytes());
    encoder.field(7, &receipt.recovered_ts().timestamp_millis().to_be_bytes());
    encoder.field(8, &(receipt.replayed_events() as u64).to_be_bytes());
    encoder.field(9, &(receipt.duplicate_events() as u64).to_be_bytes());
    encoder.finish()
}

pub(crate) struct Stage5eObservedLiveBarWithSequenceEvidence {
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    accepted_semantic_bar: Stage5cAcceptedSemanticBar,
    classified_sequence: Stage5cClassifiedSequenceSeal,
}

impl Stage5eObservedLiveBarWithSequenceEvidence {
    pub(crate) fn preflight_for_b3b(
        &self,
        seal: crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eB3bPreflightSeal,
    ) -> crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eB3bObservedLiveBarPreflight<'_>
    {
        let candidate = &self.classified_sequence.candidate;
        crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eB3bObservedLiveBarPreflight::from_stage5c_observed(
            seal,
            &self.strategy,
            &self.recovery_receipt,
            &self.accepted_semantic_bar,
            self.accepted_semantic_bar.semantic_bar_identity,
            &candidate.instrument,
            candidate.current_close_ts,
            candidate.predecessor_close_ts,
            &self.classified_sequence.returned_projection,
            self.classified_sequence.classification,
            self.classified_sequence.boundary_fingerprint,
            self.classified_sequence.sequence_identity_fingerprint,
            candidate.sequence_observed_at,
            candidate.sequence_expires_at,
        )
    }

    pub(crate) fn consume_for_b3b(
        self,
        seal: crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eB3bConsumeSeal,
    ) -> crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eB3bObservedLiveBarBridgePayload
    {
        let Stage5cClassifiedSequenceSeal {
            candidate,
            classification,
            boundary_fingerprint,
            sequence_identity_fingerprint,
            returned_projection,
        } = self.classified_sequence;
        let instrument = candidate.instrument;
        let predecessor_close_ts = candidate.predecessor_close_ts;
        let current_close_ts = candidate.current_close_ts;
        let sequence_observed_at = candidate.sequence_observed_at;
        let sequence_expires_at = candidate.sequence_expires_at;
        let accepted_semantic_bar_identity = self.accepted_semantic_bar.semantic_bar_identity;
        crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eB3bObservedLiveBarBridgePayload::from_stage5c_observed(
            seal,
            self.strategy,
            self.recovery_receipt,
            self.accepted_semantic_bar,
            accepted_semantic_bar_identity,
            instrument,
            current_close_ts,
            predecessor_close_ts,
            returned_projection,
            classification,
            boundary_fingerprint,
            sequence_identity_fingerprint,
            sequence_observed_at,
            sequence_expires_at,
        )
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage5eSequenceObservationBlockReason {
    Contextual(crate::stage5e_no_io_lifecycle::Stage5eContextualAdmissionError),
    InvalidTimeframe,
    RecoveryObservedInFuture,
    SequenceObservationTooLate,
    SequenceAlreadyExpired,
    Schedule(
        crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eScheduleClassificationBlockReason,
    ),
}

#[allow(dead_code)]
pub(crate) struct Stage5eObservedLiveBarWithSequenceEvidenceBlocked {
    reason: Stage5eSequenceObservationBlockReason,
    recovered: Stage5cPendingRecoveredPaperStrategy,
    accepted: Stage5cAcceptedSemanticBar,
    returned_projection:
        crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eScheduleProjectionBridgeInput,
}

#[allow(dead_code)]
impl Stage5eObservedLiveBarWithSequenceEvidenceBlocked {
    pub(crate) fn reason(&self) -> Stage5eSequenceObservationBlockReason {
        self.reason
    }

    pub(crate) fn into_retry(
        self,
    ) -> (
        Stage5cPendingRecoveredPaperStrategy,
        Stage5cAcceptedSemanticBar,
        crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eScheduleProjectionBridgeInput,
    ){
        (self.recovered, self.accepted, self.returned_projection)
    }
}

#[allow(dead_code)] // No runtime/live caller is authorized in this stage.
pub(crate) fn stage5e_try_observe_live_bar_after_history_with_sequence_evidence(
    recovered: Stage5cPendingRecoveredPaperStrategy,
    accepted: Stage5cAcceptedSemanticBar,
    projection: crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eScheduleProjectionBridgeInput,
) -> Result<
    Stage5eObservedLiveBarWithSequenceEvidence,
    Box<Stage5eObservedLiveBarWithSequenceEvidenceBlocked>,
> {
    stage5e_try_observe_live_bar_after_history_with_sequence_evidence_with_clock(
        recovered,
        accepted,
        projection,
        Utc::now(),
    )
}

#[cfg(test)]
fn stage5e_try_observe_live_bar_after_history_with_sequence_evidence_at(
    recovered: Stage5cPendingRecoveredPaperStrategy,
    accepted: Stage5cAcceptedSemanticBar,
    projection: crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eScheduleProjectionBridgeInput,
    sequence_observed_at: DateTime<Utc>,
) -> Result<
    Stage5eObservedLiveBarWithSequenceEvidence,
    Box<Stage5eObservedLiveBarWithSequenceEvidenceBlocked>,
> {
    stage5e_try_observe_live_bar_after_history_with_sequence_evidence_with_clock(
        recovered,
        accepted,
        projection,
        sequence_observed_at,
    )
}

#[allow(dead_code)] // Production issuer plus cfg(test)-only deterministic clock seam.
fn stage5e_try_observe_live_bar_after_history_with_sequence_evidence_with_clock(
    recovered: Stage5cPendingRecoveredPaperStrategy,
    accepted: Stage5cAcceptedSemanticBar,
    projection: crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eScheduleProjectionBridgeInput,
    sequence_observed_at: DateTime<Utc>,
) -> Result<
    Stage5eObservedLiveBarWithSequenceEvidence,
    Box<Stage5eObservedLiveBarWithSequenceEvidenceBlocked>,
> {
    let admission = &recovered
        .receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    if let Err(reason) = crate::stage5e_no_io_lifecycle::validate_contextual_live_bar_after_history(
        accepted.origin,
        &accepted.bar.instrument,
        admission.target_instrument(),
        accepted.tick_size,
        admission.tick_size(),
        recovered.receipt.warmup_receipt().last_history_ts(),
        accepted.bar.close_time_utc,
        admission.expires_at(),
        sequence_observed_at,
    ) {
        return Err(Box::new(
            Stage5eObservedLiveBarWithSequenceEvidenceBlocked {
                reason: Stage5eSequenceObservationBlockReason::Contextual(reason),
                recovered,
                accepted,
                returned_projection: projection,
            },
        ));
    }
    let Some(timeframe_sec) = std::num::NonZeroU32::new(accepted.bar.timeframe_sec) else {
        return Err(Box::new(
            Stage5eObservedLiveBarWithSequenceEvidenceBlocked {
                reason: Stage5eSequenceObservationBlockReason::InvalidTimeframe,
                recovered,
                accepted,
                returned_projection: projection,
            },
        ));
    };
    if recovered.receipt.recovered_ts() > sequence_observed_at {
        return Err(Box::new(
            Stage5eObservedLiveBarWithSequenceEvidenceBlocked {
                reason: Stage5eSequenceObservationBlockReason::RecoveryObservedInFuture,
                recovered,
                accepted,
                returned_projection: projection,
            },
        ));
    }
    let sequence_age_sec = sequence_observed_at
        .timestamp()
        .checked_sub(accepted.bar.close_time_utc)
        .unwrap_or(i64::MAX);
    if sequence_age_sec < 0 || sequence_age_sec > i64::from(timeframe_sec.get()) {
        return Err(Box::new(
            Stage5eObservedLiveBarWithSequenceEvidenceBlocked {
                reason: Stage5eSequenceObservationBlockReason::SequenceObservationTooLate,
                recovered,
                accepted,
                returned_projection: projection,
            },
        ));
    }
    let ttl_expiry = sequence_observed_at
        .checked_add_signed(chrono::Duration::seconds(i64::from(timeframe_sec.get())))
        .unwrap_or(DateTime::<Utc>::MAX_UTC);
    let sequence_expires_at = admission.expires_at().min(ttl_expiry);
    let Some(candidate) =
        build_stage5c_sequence_candidate_seal_inside_stage5e_try_observe_live_bar_after_history_with_sequence_evidence(
        accepted.bar.instrument.clone(),
        recovered.receipt.warmup_receipt().last_history_ts(),
        accepted.bar.close_time_utc,
        timeframe_sec,
        accepted.stage3_provenance_identity,
        accepted.semantic_bar_identity,
        stage5e_b3c_recovery_receipt_identity(&recovered.receipt),
        sequence_observed_at,
        sequence_expires_at,
    )
    else {
        return Err(Box::new(Stage5eObservedLiveBarWithSequenceEvidenceBlocked {
            reason: Stage5eSequenceObservationBlockReason::SequenceAlreadyExpired,
            recovered,
            accepted,
            returned_projection: projection,
        }));
    };
    let classifier = crate::stage5e_no_io_lifecycle::schedule_window_evidence::into_stage5e_schedule_candidate_classifier(projection);
    let classified_sequence = match candidate.classify_with_owned_projection(classifier) {
        Ok(classified) => classified,
        Err(blocked) => {
            let reason = blocked.reason();
            return Err(Box::new(
                Stage5eObservedLiveBarWithSequenceEvidenceBlocked {
                    reason: Stage5eSequenceObservationBlockReason::Schedule(reason),
                    recovered,
                    accepted,
                    returned_projection: blocked.into_retry(),
                },
            ));
        }
    };
    let (strategy, recovery_receipt) = recovered.into_parts();
    Ok(Stage5eObservedLiveBarWithSequenceEvidence {
        strategy,
        recovery_receipt,
        accepted_semantic_bar: accepted,
        classified_sequence,
    })
}

#[allow(dead_code)] // The consumer remains closed outside the Stage 5E test-only proof.
pub(crate) enum Stage5eNoIoLiveBarAfterHistoryBlocked {
    Contextual {
        reason: crate::stage5e_no_io_lifecycle::Stage5eContextualAdmissionError,
        recovered: Box<Stage5cPendingRecoveredPaperStrategy>,
        accepted: Box<Stage5cAcceptedSemanticBar>,
    },
}

#[allow(dead_code)] // Retry API is retained for the next separately reviewed consumer slice.
impl Stage5eNoIoLiveBarAfterHistoryBlocked {
    pub(crate) fn reason(&self) -> crate::stage5e_no_io_lifecycle::Stage5eContextualAdmissionError {
        match self {
            Self::Contextual { reason, .. } => *reason,
        }
    }

    pub(crate) fn into_retry(
        self,
    ) -> (
        Stage5cPendingRecoveredPaperStrategy,
        Stage5cAcceptedSemanticBar,
    ) {
        match self {
            Self::Contextual {
                recovered,
                accepted,
                ..
            } => (*recovered, *accepted),
        }
    }
}

#[allow(dead_code)] // Stage 5E-b1 consumer remains deliberately closed.
pub(crate) fn stage5e_try_observe_live_bar_after_history(
    recovered: Stage5cPendingRecoveredPaperStrategy,
    accepted: Stage5cAcceptedSemanticBar,
) -> Result<
    crate::stage5e_no_io_lifecycle::Stage5eObservedLiveBarAfterHistory,
    Stage5eNoIoLiveBarAfterHistoryBlocked,
> {
    stage5e_try_observe_live_bar_after_history_with_lifecycle_now(recovered, accepted, Utc::now())
}

#[cfg(test)]
pub(crate) fn stage5e_try_observe_live_bar_after_history_at(
    recovered: Stage5cPendingRecoveredPaperStrategy,
    accepted: Stage5cAcceptedSemanticBar,
    lifecycle_now: DateTime<Utc>,
) -> Result<
    crate::stage5e_no_io_lifecycle::Stage5eObservedLiveBarAfterHistory,
    Stage5eNoIoLiveBarAfterHistoryBlocked,
> {
    stage5e_try_observe_live_bar_after_history_with_lifecycle_now(
        recovered,
        accepted,
        lifecycle_now,
    )
}

fn stage5e_try_observe_live_bar_after_history_with_lifecycle_now(
    recovered: Stage5cPendingRecoveredPaperStrategy,
    accepted: Stage5cAcceptedSemanticBar,
    lifecycle_now: DateTime<Utc>,
) -> Result<
    crate::stage5e_no_io_lifecycle::Stage5eObservedLiveBarAfterHistory,
    Stage5eNoIoLiveBarAfterHistoryBlocked,
> {
    // Recovery receipt establishes causality/ownership only. Market-bar time is
    // compared solely to canonical-history time in the Stage 5E boundary.
    let (target_instrument, admission_tick_size, admission_expires_at) = {
        let admission = &recovered
            .receipt
            .warmup_receipt()
            .restore_receipt()
            .bootstrap_receipt()
            .admission;
        (
            admission.target_instrument().clone(),
            admission.tick_size(),
            admission.expires_at(),
        )
    };
    if let Err(reason) = crate::stage5e_no_io_lifecycle::validate_contextual_live_bar_after_history(
        accepted.origin,
        &accepted.bar.instrument,
        &target_instrument,
        accepted.tick_size,
        admission_tick_size,
        recovered.receipt.warmup_receipt().last_history_ts(),
        accepted.bar.close_time_utc,
        admission_expires_at,
        lifecycle_now,
    ) {
        return Err(Stage5eNoIoLiveBarAfterHistoryBlocked::Contextual {
            reason,
            recovered: Box::new(recovered),
            accepted: Box::new(accepted),
        });
    }
    let (strategy, recovery_receipt) = recovered.into_parts();
    Ok(
        crate::stage5e_no_io_lifecycle::Stage5eObservedLiveBarAfterHistory::from_stage5c_context(
            Stage5eNoIoBridgeSeal(()),
            strategy,
            recovery_receipt,
            accepted.bar,
            accepted.tick_size,
        ),
    )
}

#[cfg(test)]
mod stage5e_retryable_bridge_tests {
    use super::*;
    use rust_decimal::Decimal;

    fn target() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: broker_core::Exchange::Moex,
            market: broker_core::Market::Futures,
        }
    }

    fn strategy() -> HybridIntradayRuntimeStrategy {
        HybridIntradayRuntimeStrategy::new(
            crate::hybrid_intraday_runtime::HybridIntradayRuntimeConfig {
                symbol: "IMOEXF".to_string(),
                profile:
                    crate::hybrid_intraday_runtime::HybridIntradayProfile::BaselineRuntimeHybrid,
                mr_variant:
                    crate::hybrid_intraday_runtime::MeanReversionVariant::ClassicPrevDayRange,
                mr_gate_policy: crate::hybrid_intraday_runtime::MrGatePolicy::Disabled,
                risk_gate_mode: crate::hybrid_intraday_runtime::RiskGateMode::Disabled,
                risk_gate_seed_file: None,
                risk_gate_ledger_key: None,
                model_session_start_time: None,
                model_session_end_time: None,
                qty: 1.0,
                live_order_style: crate::runtime_compat::MarketBuyAndCloseLiveOrderStyle::Market,
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
                pending_timeout_sec: 30,
                partial_entry_fill_timeout_ms: 3_000,
                mr_config: crate::hybrid_intraday::MeanReversionConfig::default(),
                breakout_config: crate::hybrid_intraday::IntradayBreakoutConfig::default(),
                orchestrator_config: crate::hybrid_intraday::HybridOrchestratorConfig::default(),
            },
        )
    }

    fn recovered(now: DateTime<Utc>, last_history_ts: i64) -> Stage5cPendingRecoveredPaperStrategy {
        let admission = Stage5cPaperHostAdmission::stage5d_test_new(
            "stage5e_test".to_string(),
            BrokerAccountId::new("ACC_TEST_0001"),
            target(),
            0.5,
            Decimal::ZERO,
            now,
        );
        let bootstrap = Stage5cBootstrapNotificationReceipt {
            admission,
            notified_ts: now,
        };
        let restore = Stage5cRuntimeStateRestoreReceipt {
            bootstrap_receipt: bootstrap,
            restored_ts: now,
            known_order_ids: Vec::new(),
            pending_requests: Vec::new(),
        };
        let warmup = Stage5cHistoryWarmupReceipt {
            restore_receipt: restore,
            started_ts: now,
            processed_bars: 1,
            input_bars: 1,
            source_mode: broker_core::Stage3StrategyBarSourceMode::FinamDerivedM1ToM10,
            last_history_ts,
        };
        Stage5cPendingRecoveredPaperStrategy {
            strategy: strategy(),
            receipt: Stage5cPendingRecoveryReceipt {
                warmup_receipt: warmup,
                recovered_ts: now,
                replayed_events: 0,
                duplicate_events: 0,
            },
        }
    }

    fn accepted(
        origin: broker_core::HybridRuntimeBarOrigin,
        close_time_utc: i64,
    ) -> Stage5cAcceptedSemanticBar {
        let bar = broker_core::HybridRuntimeBarEvent {
            instrument: target(),
            close_time_utc,
            open: 2200.0,
            high: 2202.0,
            low: 2199.0,
            close: 2201.0,
            volume: 1.0,
            origin,
            is_final: true,
            timeframe_sec: 600,
        };
        let provenance =
            broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete();
        let stage3_provenance_identity = stage5e_b3c_stage3_provenance_identity(&provenance);
        let semantic_bar_identity =
            stage5e_b3c_semantic_bar_identity(&bar, stage3_provenance_identity);
        Stage5cAcceptedSemanticBar {
            bar,
            tick_size: 0.5,
            origin,
            stage3_provenance_identity,
            semantic_bar_identity,
        }
    }

    pub(super) fn canonical_observed_live_bar_after_history(
        now: DateTime<Utc>,
        bar_close_ts: i64,
    ) -> crate::stage5e_no_io_lifecycle::Stage5eObservedLiveBarAfterHistory {
        match stage5e_try_observe_live_bar_after_history_at(
            recovered(now, bar_close_ts - 600),
            accepted(broker_core::HybridRuntimeBarOrigin::Live, bar_close_ts),
            now,
        ) {
            Ok(observed) => observed,
            Err(_) => panic!("canonical Stage 5C recovery chain must admit its first live bar"),
        }
    }

    pub(super) fn canonical_sequence_inputs(
        now: DateTime<Utc>,
        predecessor_close_ts: i64,
        current_close_ts: i64,
    ) -> (
        Stage5cPendingRecoveredPaperStrategy,
        Stage5cAcceptedSemanticBar,
    ) {
        (
            recovered(now, predecessor_close_ts),
            accepted(broker_core::HybridRuntimeBarOrigin::Live, current_close_ts),
        )
    }

    #[test]
    fn stage5e_real_bridge_returns_retryable_recovered_state_then_accepts_next_live_bar() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 23, 10, 30, 0)
            .single()
            .unwrap();
        let history_close = now.timestamp() - 1_200;
        let blocked = match stage5e_try_observe_live_bar_after_history_at(
            recovered(now, history_close),
            accepted(
                broker_core::HybridRuntimeBarOrigin::Replay,
                now.timestamp() - 600,
            ),
            now,
        ) {
            Ok(_) => panic!("replay must remain observation-blocked"),
            Err(blocked) => blocked,
        };
        assert_eq!(
            blocked.reason(),
            crate::stage5e_no_io_lifecycle::Stage5eContextualAdmissionError::NotLive
        );
        let (recovered, _rejected) = blocked.into_retry();
        let observed = match stage5e_try_observe_live_bar_after_history_at(
            recovered,
            accepted(broker_core::HybridRuntimeBarOrigin::Live, now.timestamp()),
            now,
        ) {
            Ok(observed) => observed,
            Err(_) => panic!("next live candidate must be accepted once"),
        };
        assert_eq!(observed.bar_close_ts(), now.timestamp());
        assert_eq!(observed.callback_count(), 0);
        assert_eq!(observed.intent_count(), 0);
        assert!(!observed.strategy_was_called());
        assert!(!observed.executable_intent_created());
    }
}

/// Test-only fixture for a real, fully-owned Stage 5C recovery chain.  It is
/// deliberately not an observed-bar constructor: it must pass through the
/// sealed Stage 5C bridge and therefore retains strategy and recovery state.
#[cfg(test)]
pub(crate) fn stage5e_test_observed_live_bar_after_history_at(
    now: DateTime<Utc>,
    bar_close_ts: i64,
) -> crate::stage5e_no_io_lifecycle::Stage5eObservedLiveBarAfterHistory {
    stage5e_retryable_bridge_tests::canonical_observed_live_bar_after_history(now, bar_close_ts)
}

#[cfg(test)]
pub(crate) fn stage5e_test_sequence_inputs(
    now: DateTime<Utc>,
    predecessor_close_ts: i64,
    current_close_ts: i64,
) -> (
    Stage5cPendingRecoveredPaperStrategy,
    Stage5cAcceptedSemanticBar,
) {
    stage5e_retryable_bridge_tests::canonical_sequence_inputs(
        now,
        predecessor_close_ts,
        current_close_ts,
    )
}

// STAGE5F-TEST-OWNERSHIP-FACTORY-BEGIN
#[cfg(test)]
pub(crate) mod stage5f_test_seams {
    use super::*;

    /// Carries a fixture-owned strategy into the existing linear Stage 5C
    /// wrappers without adding a production constructor or callback bypass.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn sequence_inputs_from_owned_strategy(
        strategy: HybridIntradayRuntimeStrategy,
        strategy_id: String,
        account_id: BrokerAccountId,
        target: InstrumentId,
        tick_size: f64,
        position_qty: rust_decimal::Decimal,
        lifecycle_now: DateTime<Utc>,
        predecessor_close_ts: i64,
        bar: broker_core::HybridRuntimeBarEvent,
    ) -> (
        Stage5cPendingRecoveredPaperStrategy,
        Stage5cAcceptedSemanticBar,
    ) {
        assert_eq!(bar.instrument, target, "Stage 5F target/bar identity drift");
        assert_eq!(bar.origin, broker_core::HybridRuntimeBarOrigin::Live);
        assert!(bar.is_final);
        assert_eq!(bar.timeframe_sec, 600);
        assert_eq!(bar.close_time_utc - predecessor_close_ts, 600);

        let admission = Stage5cPaperHostAdmission::stage5d_test_new(
            strategy_id,
            account_id,
            target,
            tick_size,
            position_qty,
            lifecycle_now,
        );
        let recovery_receipt = Stage5cPendingRecoveryReceipt {
            warmup_receipt: Stage5cHistoryWarmupReceipt {
                restore_receipt: Stage5cRuntimeStateRestoreReceipt {
                    bootstrap_receipt: Stage5cBootstrapNotificationReceipt {
                        admission,
                        notified_ts: lifecycle_now,
                    },
                    restored_ts: lifecycle_now,
                    known_order_ids: Vec::new(),
                    pending_requests: Vec::new(),
                },
                started_ts: lifecycle_now,
                processed_bars: 1,
                input_bars: 1,
                source_mode: broker_core::Stage3StrategyBarSourceMode::FinamDerivedM1ToM10,
                last_history_ts: predecessor_close_ts,
            },
            recovered_ts: lifecycle_now,
            replayed_events: 0,
            duplicate_events: 0,
        };
        let provenance =
            broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete();
        let stage3_provenance_identity = stage5e_b3c_stage3_provenance_identity(&provenance);
        let semantic_bar_identity =
            stage5e_b3c_semantic_bar_identity(&bar, stage3_provenance_identity);
        (
            Stage5cPendingRecoveredPaperStrategy {
                strategy,
                receipt: recovery_receipt,
            },
            Stage5cAcceptedSemanticBar {
                bar,
                tick_size,
                origin: broker_core::HybridRuntimeBarOrigin::Live,
                stage3_provenance_identity,
                semantic_bar_identity,
            },
        )
    }

    /// Continues an actual Stage 5D-restored capability through the production
    /// Stage 5C history, pending-recovery and semantic-bar validators. The
    /// recovery evidence is an explicit empty paper claim; no Redis is opened.
    pub(crate) fn sequence_inputs_from_restored_strategy(
        restored: Stage5cRuntimeStateRestoredPaperStrategy,
        lifecycle_now: DateTime<Utc>,
        bar: broker_core::HybridRuntimeBarEvent,
    ) -> (
        Stage5cPendingRecoveredPaperStrategy,
        Stage5cAcceptedSemanticBar,
    ) {
        assert_eq!(bar.origin, broker_core::HybridRuntimeBarOrigin::Live);
        assert!(bar.is_final);
        assert_eq!(bar.timeframe_sec, 600);
        let mut predecessor = bar.clone();
        predecessor.origin = broker_core::HybridRuntimeBarOrigin::History;
        predecessor.close_time_utc -= 600;
        predecessor.open = bar.open;
        predecessor.high = bar.open.max(bar.close);
        predecessor.low = bar.open.min(bar.close);
        predecessor.close = bar.open;
        predecessor.volume = 1.0;
        let provenance =
            broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete();
        let history = accept_stage5c_history_batch(Stage5cHistoryBatchInput {
            bars: vec![predecessor],
            provenance: provenance.clone(),
        })
        .expect("Stage 5F representative history must validate");
        let warmed = warmup_stage5c_history_at(restored, history, lifecycle_now)
            .expect("Stage 5F representative history must warm the restored strategy");

        let admission = &warmed
            .receipt()
            .restore_receipt()
            .bootstrap_receipt()
            .admission;
        let strategy_id = admission.strategy_id().to_string();
        let account_id = admission.account_id().clone();
        let target_instrument = admission.target_instrument().clone();
        let snapshot_received_ts = admission.bootstrap_snapshot().received_ts;
        let consumer_group = format!("paper-runtime:{account_id}:{strategy_id}");
        let streams = [
            Stage5cPendingStreamKind::Ack,
            Stage5cPendingStreamKind::Order,
            Stage5cPendingStreamKind::StopOrder,
            Stage5cPendingStreamKind::Position,
        ]
        .into_iter()
        .map(|stream_kind| Stage5cPendingStreamClaimBoundary {
            stream_name: canonical_pending_stream_name(stream_kind, &account_id),
            stream_kind,
            consumer_group: consumer_group.clone(),
            terminal_claim_cursor: "0-0".to_string(),
            snapshot_boundary_entry_id: "0-0".to_string(),
            claimed_count: 0,
        })
        .collect();
        let claim_proof = prove_stage5c_pending_recovery_claim(
            &warmed,
            Stage5cPendingRecoveryClaimProofInput {
                strategy_id,
                account_id,
                target_instrument,
                snapshot_received_ts,
                completed_ts: lifecycle_now,
                streams,
            },
        )
        .expect("Stage 5F representative empty pending claim must validate");
        let evidence =
            accept_stage5c_pending_recovery_evidence(Stage5cPendingRecoveryEvidenceInput {
                events: Vec::new(),
                claim_proof,
            })
            .expect("Stage 5F representative empty pending evidence must validate");
        let recovered = recover_stage5c_pending_streams_at(warmed, evidence, lifecycle_now)
            .expect("Stage 5F representative pending recovery must complete");
        let accepted = accept_stage5c_semantic_bar(Stage5cSemanticBarInput {
            bar,
            provenance,
            tick_size: 0.5,
        })
        .expect("Stage 5F representative semantic bar must validate");
        (recovered, accepted)
    }
}
// STAGE5F-TEST-OWNERSHIP-FACTORY-END

#[cfg(test)]
pub(crate) fn stage5e_test_pending_recovered_state_fingerprint(
    recovered: &Stage5cPendingRecoveredPaperStrategy,
) -> String {
    stage5c_state_fingerprint(Strategy::state(recovered.strategy()))
}

#[cfg(test)]
pub(crate) fn stage5e_test_owned_strategy_state_fingerprint(
    strategy: &HybridIntradayRuntimeStrategy,
) -> String {
    stage5c_state_fingerprint(Strategy::state(strategy))
}

#[cfg(test)]
pub(crate) fn stage5e_test_observe_live_bar_with_sequence_evidence_at(
    recovered: Stage5cPendingRecoveredPaperStrategy,
    accepted: Stage5cAcceptedSemanticBar,
    projection: crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eScheduleProjectionBridgeInput,
    now: DateTime<Utc>,
) -> Result<
    Stage5eObservedLiveBarWithSequenceEvidence,
    Box<Stage5eObservedLiveBarWithSequenceEvidenceBlocked>,
> {
    stage5e_try_observe_live_bar_after_history_with_sequence_evidence_at(
        recovered, accepted, projection, now,
    )
}

#[cfg(test)]
mod stage5e_b3c_identity_tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
            .single()
            .unwrap()
    }

    fn bar() -> broker_core::HybridRuntimeBarEvent {
        broker_core::HybridRuntimeBarEvent {
            instrument: InstrumentId {
                symbol: "IMOEXF".to_owned(),
                venue_symbol: Some("IMOEXF@RTSX".to_owned()),
                exchange: broker_core::Exchange::Moex,
                market: broker_core::Market::Futures,
            },
            close_time_utc: now().timestamp(),
            open: 2200.0,
            high: 2202.0,
            low: 2199.0,
            close: 2201.0,
            volume: 42.0,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            is_final: true,
            timeframe_sec: 600,
        }
    }

    #[test]
    fn stage3_identity_changes_for_every_frozen_field() {
        let base = broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete();
        let expected = stage5e_b3c_stage3_provenance_identity(&base);
        let mutations = [
            broker_core::Stage3StrategyBarProvenance {
                source_mode:
                    broker_core::Stage3StrategyBarSourceMode::AlorNativeBarsGetAndSubscribeTf600,
                ..base.clone()
            },
            broker_core::Stage3StrategyBarProvenance {
                source_timeframe_sec: None,
                ..base.clone()
            },
            broker_core::Stage3StrategyBarProvenance {
                target_timeframe_sec: 1_200,
                ..base.clone()
            },
            broker_core::Stage3StrategyBarProvenance {
                aggregation_complete: false,
                ..base.clone()
            },
            broker_core::Stage3StrategyBarProvenance {
                gap_absence_proven: false,
                ..base
            },
        ];
        for mutation in mutations {
            assert_ne!(stage5e_b3c_stage3_provenance_identity(&mutation), expected);
        }
    }

    #[test]
    fn semantic_bar_identity_changes_for_every_frozen_field() {
        let provenance =
            broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete();
        let stage3 = stage5e_b3c_stage3_provenance_identity(&provenance);
        let base = bar();
        let expected = stage5e_b3c_semantic_bar_identity(&base, stage3);
        let mut mutations = Vec::new();
        let mut value = base.clone();
        value.instrument.symbol = "OTHER".to_owned();
        mutations.push((value, stage3));
        let mut value = base.clone();
        value.instrument.venue_symbol = Some("IMOEXF@OTHER".to_owned());
        mutations.push((value, stage3));
        let mut value = base.clone();
        value.instrument.exchange = broker_core::Exchange::Other("other".to_owned());
        mutations.push((value, stage3));
        let mut value = base.clone();
        value.instrument.market = broker_core::Market::Other("other".to_owned());
        mutations.push((value, stage3));
        let mut value = base.clone();
        value.close_time_utc += 600;
        mutations.push((value, stage3));
        for field in 0..5 {
            let mut value = base.clone();
            match field {
                0 => value.open += 0.5,
                1 => value.high += 0.5,
                2 => value.low -= 0.5,
                3 => value.close += 0.5,
                _ => value.volume += 1.0,
            }
            mutations.push((value, stage3));
        }
        let mut value = base.clone();
        value.origin = broker_core::HybridRuntimeBarOrigin::Replay;
        mutations.push((value, stage3));
        let mut value = base.clone();
        value.is_final = false;
        mutations.push((value, stage3));
        let mut value = base.clone();
        value.timeframe_sec = 1_200;
        mutations.push((value, stage3));
        mutations.push((base.clone(), [99; 32]));
        for (mutation, provenance_identity) in mutations {
            assert_ne!(
                stage5e_b3c_semantic_bar_identity(&mutation, provenance_identity),
                expected
            );
        }
    }

    #[test]
    fn recovery_identity_changes_for_every_frozen_field() {
        let build = || {
            stage5e_retryable_bridge_tests::canonical_sequence_inputs(
                now(),
                now().timestamp() - 600,
                now().timestamp(),
            )
            .0
        };
        let expected = stage5e_b3c_recovery_receipt_identity(&build().receipt);

        let mut changed = build();
        changed.receipt.warmup_receipt.restore_receipt.restored_ts +=
            chrono::Duration::milliseconds(1);
        assert_ne!(
            stage5e_b3c_recovery_receipt_identity(&changed.receipt),
            expected
        );
        let mut changed = build();
        changed.receipt.warmup_receipt.started_ts += chrono::Duration::milliseconds(1);
        assert_ne!(
            stage5e_b3c_recovery_receipt_identity(&changed.receipt),
            expected
        );
        let mut changed = build();
        changed.receipt.warmup_receipt.processed_bars += 1;
        assert_ne!(
            stage5e_b3c_recovery_receipt_identity(&changed.receipt),
            expected
        );
        let mut changed = build();
        changed.receipt.warmup_receipt.input_bars += 1;
        assert_ne!(
            stage5e_b3c_recovery_receipt_identity(&changed.receipt),
            expected
        );
        let mut changed = build();
        changed.receipt.warmup_receipt.source_mode =
            broker_core::Stage3StrategyBarSourceMode::AlorStandDerivedM1ToM10;
        assert_ne!(
            stage5e_b3c_recovery_receipt_identity(&changed.receipt),
            expected
        );
        let mut changed = build();
        changed.receipt.warmup_receipt.last_history_ts -= 600;
        assert_ne!(
            stage5e_b3c_recovery_receipt_identity(&changed.receipt),
            expected
        );
        let mut changed = build();
        changed.receipt.recovered_ts += chrono::Duration::milliseconds(1);
        assert_ne!(
            stage5e_b3c_recovery_receipt_identity(&changed.receipt),
            expected
        );
        let mut changed = build();
        changed.receipt.replayed_events += 1;
        assert_ne!(
            stage5e_b3c_recovery_receipt_identity(&changed.receipt),
            expected
        );
        let mut changed = build();
        changed.receipt.duplicate_events += 1;
        assert_ne!(
            stage5e_b3c_recovery_receipt_identity(&changed.receipt),
            expected
        );
    }

    #[test]
    fn final_sequence_identity_changes_for_every_frozen_field() {
        #[derive(Clone)]
        struct Material {
            stage3: [u8; 32],
            semantic: [u8; 32],
            recovery: [u8; 32],
            predecessor: i64,
            timeframe: std::num::NonZeroU32,
            observed: DateTime<Utc>,
            expires: DateTime<Utc>,
            class: u8,
            boundary: Option<[u8; 32]>,
        }
        impl Material {
            fn identity(&self) -> [u8; 32] {
                stage5e_b3c_final_sequence_identity(
                    self.stage3,
                    self.semantic,
                    self.recovery,
                    self.predecessor,
                    self.timeframe,
                    self.observed,
                    self.expires,
                    self.class,
                    self.boundary,
                )
            }
        }
        let base = Material {
            stage3: [1; 32],
            semantic: [2; 32],
            recovery: [3; 32],
            predecessor: now().timestamp() - 600,
            timeframe: std::num::NonZeroU32::new(600).unwrap(),
            observed: now(),
            expires: now() + chrono::Duration::seconds(600),
            class: 1,
            boundary: None,
        };
        let expected = base.identity();
        let mut mutations = Vec::new();
        let mut value = base.clone();
        value.stage3 = [11; 32];
        mutations.push(value);
        let mut value = base.clone();
        value.semantic = [12; 32];
        mutations.push(value);
        let mut value = base.clone();
        value.recovery = [13; 32];
        mutations.push(value);
        let mut value = base.clone();
        value.predecessor -= 600;
        mutations.push(value);
        let mut value = base.clone();
        value.timeframe = std::num::NonZeroU32::new(1_200).unwrap();
        mutations.push(value);
        let mut value = base.clone();
        value.observed += chrono::Duration::milliseconds(1);
        mutations.push(value);
        let mut value = base.clone();
        value.expires += chrono::Duration::milliseconds(1);
        mutations.push(value);
        let mut value = base.clone();
        value.class = 2;
        mutations.push(value);
        let mut value = base;
        value.class = 2;
        value.boundary = Some([14; 32]);
        mutations.push(value);
        for mutation in mutations {
            assert_ne!(mutation.identity(), expected);
        }
    }
}
// STAGE5E-B3C-R6-SEALS-END: additive-no-io-v1
// STAGE5E-NO-IO-BRIDGE-END: contextual-observation-v1
// STAGE5D-ADDITIVE-BRIDGE-END: type-state-transitions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cPaperHostAdmissionError {
    Stage4ReportSchemaMismatch,
    Stage4ReportNotAccepted,
    Stage4EvidenceChainInconsistent,
    Stage4SafetyBoundaryOpen,
    Stage4ApplicationSchemaMismatch,
    Stage4ApplicationNotApplied,
    Stage4ApplicationInconsistent,
    Stage4ApplicationSnapshotMissing,
    Stage4ReportApplicationMismatch,
    TargetInstrumentMismatch,
    AccountScopeMismatch,
    InstrumentSpecMismatch,
    InvalidInstrumentPriceStep,
    TickSizeMismatch,
    LiveOrdersRequested,
    StrategyIdEmpty,
    EvidenceCheckedInFuture,
    EvidenceExpired,
}

impl std::fmt::Display for Stage5cPaperHostAdmissionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "stage 5C paper host admission blocked: {self:?}")
    }
}

impl std::error::Error for Stage5cPaperHostAdmissionError {}

/// Input accepts one opaque canonical Stage 4 chain, never independent report
/// and application DTOs.
///
/// ```compile_fail
/// # use broker_core::{BrokerAccountId, BrokerInstrumentSpec, InstrumentId, Stage4AcceptedPaperHostEvidence};
/// # use strategy_runtime_core::{admit_stage5c_paper_host, Stage5cPaperHostAdmissionInput};
/// # fn duplicate(evidence: Stage4AcceptedPaperHostEvidence, spec: &BrokerInstrumentSpec, account: &BrokerAccountId, target: &InstrumentId) {
/// let _ = admit_stage5c_paper_host(Stage5cPaperHostAdmissionInput {
///     stage4_evidence: evidence,
///     strategy_id: "hybrid_imoexf".to_string(),
///     instrument_spec: spec,
///     configured_account_id: account,
///     configured_target_instrument: target,
///     configured_tick_size: 0.5,
///     allow_live_orders: false,
/// });
/// let _ = admit_stage5c_paper_host(Stage5cPaperHostAdmissionInput {
///     stage4_evidence: evidence,
///     strategy_id: "hybrid_imoexf".to_string(),
///     instrument_spec: spec,
///     configured_account_id: account,
///     configured_target_instrument: target,
///     configured_tick_size: 0.5,
///     allow_live_orders: false,
/// });
/// # }
/// ```
pub struct Stage5cPaperHostAdmissionInput<'a> {
    pub stage4_evidence: Stage4AcceptedPaperHostEvidence,
    pub strategy_id: String,
    pub instrument_spec: &'a BrokerInstrumentSpec,
    pub configured_account_id: &'a BrokerAccountId,
    pub configured_target_instrument: &'a InstrumentId,
    pub configured_tick_size: f64,
    pub allow_live_orders: bool,
}

/// Opaque paper-host capability issued only by [`admit_stage5c_paper_host`].
///
/// It cannot be reconstructed from serialized evidence.
///
/// ```compile_fail
/// let _: strategy_runtime_core::Stage5cPaperHostAdmission =
///     serde_json::from_str("{}").unwrap();
/// ```
#[derive(PartialEq)]
pub struct Stage5cPaperHostAdmission {
    schema_version: u16,
    checked_ts: DateTime<Utc>,
    issued_ts: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    strategy_id: String,
    account_id: BrokerAccountId,
    target_instrument: InstrumentId,
    tick_size: f64,
    bootstrap_snapshot: RuntimeHostBootstrapSnapshot,
    paper_only: bool,
    runtime_host_attached: bool,
    intent_sink_attached: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cBootstrapNotificationError {
    AdmissionExpired,
    StrategyTargetMismatch,
    StrategyTickSizeMismatch,
    ActiveOrdersRequireOwnershipMapping,
    SnapshotAccountMismatch,
    SnapshotInstrumentMismatch,
    PositionQuantityNotRepresentable,
    PositionAveragePriceNotRepresentable,
}

impl std::fmt::Display for Stage5cBootstrapNotificationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Stage 5C bootstrap notification blocked: {self:?}"
        )
    }
}

impl std::error::Error for Stage5cBootstrapNotificationError {}

/// One-shot proof that only `NotifyBootstrapSnapshot` has completed.
/// Subsequent lifecycle gates must consume this receipt by value.
pub struct Stage5cBootstrapNotificationReceipt {
    admission: Stage5cPaperHostAdmission,
    notified_ts: DateTime<Utc>,
}

impl Stage5cBootstrapNotificationReceipt {
    pub fn notified_ts(&self) -> DateTime<Utc> {
        self.notified_ts
    }

    pub fn expires_at(&self) -> DateTime<Utc> {
        self.admission.expires_at()
    }

    pub fn strategy_id(&self) -> &str {
        self.admission.strategy_id()
    }

    pub fn bootstrap_snapshot(&self) -> &RuntimeHostBootstrapSnapshot {
        self.admission.bootstrap_snapshot()
    }

    pub fn runtime_state_restored(&self) -> bool {
        false
    }

    pub fn warmup_started(&self) -> bool {
        false
    }

    pub fn pending_recovery_started(&self) -> bool {
        false
    }

    pub fn semantic_bar_enabled(&self) -> bool {
        false
    }

    pub fn intent_sink_attached(&self) -> bool {
        false
    }
}

/// Linear type-state after exactly one successful bootstrap notification.
pub struct Stage5cBootstrappedPaperStrategy {
    strategy: HybridIntradayRuntimeStrategy,
    receipt: Stage5cBootstrapNotificationReceipt,
    restored: RuntimeStateRestored,
}

impl Stage5cBootstrappedPaperStrategy {
    pub fn receipt(&self) -> &Stage5cBootstrapNotificationReceipt {
        &self.receipt
    }

    #[cfg(test)]
    fn strategy(&self) -> &HybridIntradayRuntimeStrategy {
        &self.strategy
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        HybridIntradayRuntimeStrategy,
        Stage5cBootstrapNotificationReceipt,
        RuntimeStateRestored,
    ) {
        (self.strategy, self.receipt, self.restored)
    }
}

pub struct Stage5cRuntimeStateLoadedPaperStrategy {
    strategy: HybridIntradayRuntimeStrategy,
    admission: Stage5cPaperHostAdmission,
    restored: RuntimeStateRestored,
    load_origin: Stage5cRuntimeStateLoadOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cLegacyNumericOrderIdPolicy {
    Reject,
    ConvertPositiveAlorNumeric,
}

/// Persisted-state envelope supplied to the one-shot restore gate.
pub struct Stage5cRuntimeStateRestoreInput {
    pub schema_version: u16,
    pub state_schema_version: u16,
    pub strategy_kind: String,
    pub strategy_id: String,
    pub account_id: BrokerAccountId,
    pub instrument: InstrumentId,
    pub tick_size: f64,
    pub config_fingerprint: String,
    pub profile: String,
    pub mr_variant: String,
    pub mr_gate_policy: String,
    pub risk_gate_mode: String,
    pub persisted_ts: DateTime<Utc>,
    pub state_json: String,
    pub known_order_ids: Vec<BrokerOrderId>,
    pub pending_requests: Vec<StrategyRequestId>,
    pub legacy_numeric_order_id_policy: Stage5cLegacyNumericOrderIdPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cRuntimeStateRestoreError {
    SchemaMismatch,
    AdmissionExpired,
    StrategyIdMismatch,
    AccountMismatch,
    InstrumentMismatch,
    TickSizeMismatch,
    StateSchemaMismatch,
    StrategyKindMismatch,
    ConfigFingerprintMismatch,
    ProfileBindingMismatch,
    PersistedStateFromFuture,
    InvalidStateJson,
    WrongStrategyStateKind,
    LegacyNumericOrderIdRejected,
    InvalidLegacyNumericOrderId,
    BrokerTruthPositionMismatch,
    BrokerTruthSideMismatch,
    BrokerOwnedOrderIdMismatch,
}

impl std::fmt::Display for Stage5cRuntimeStateRestoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Stage 5C runtime-state restore blocked: {self:?}"
        )
    }
}

impl std::error::Error for Stage5cRuntimeStateRestoreError {}

pub struct Stage5cRuntimeStateRestoreReceipt {
    bootstrap_receipt: Stage5cBootstrapNotificationReceipt,
    restored_ts: DateTime<Utc>,
    #[allow(dead_code)]
    known_order_ids: Vec<BrokerOrderId>,
    pending_requests: Vec<StrategyRequestId>,
}

impl Stage5cRuntimeStateRestoreReceipt {
    pub fn bootstrap_receipt(&self) -> &Stage5cBootstrapNotificationReceipt {
        &self.bootstrap_receipt
    }

    pub fn restored_ts(&self) -> DateTime<Utc> {
        self.restored_ts
    }

    pub fn runtime_state_restored(&self) -> bool {
        true
    }
    pub fn pending_requests(&self) -> &[StrategyRequestId] {
        &self.pending_requests
    }

    #[cfg(test)]
    pub(crate) fn stage5d_test_known_order_ids(&self) -> &[BrokerOrderId] {
        &self.known_order_ids
    }

    pub fn warmup_started(&self) -> bool {
        false
    }

    pub fn pending_recovery_started(&self) -> bool {
        false
    }

    pub fn semantic_bar_enabled(&self) -> bool {
        false
    }

    pub fn intent_sink_attached(&self) -> bool {
        false
    }
}

/// Linear type-state after exactly one validated state restore.
pub struct Stage5cRuntimeStateRestoredPaperStrategy {
    strategy: HybridIntradayRuntimeStrategy,
    receipt: Stage5cRuntimeStateRestoreReceipt,
}

impl Stage5cRuntimeStateRestoredPaperStrategy {
    pub fn receipt(&self) -> &Stage5cRuntimeStateRestoreReceipt {
        &self.receipt
    }

    #[cfg(test)]
    fn strategy(&self) -> &HybridIntradayRuntimeStrategy {
        &self.strategy
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        HybridIntradayRuntimeStrategy,
        Stage5cRuntimeStateRestoreReceipt,
    ) {
        (self.strategy, self.receipt)
    }
}

pub struct Stage5cHistoryBatchInput {
    pub bars: Vec<broker_core::HybridRuntimeBarEvent>,
    pub provenance: broker_core::Stage3StrategyBarProvenance,
}

pub struct Stage5cAcceptedHistoryBatch {
    bars: Vec<broker_core::HybridRuntimeBarEvent>,
    provenance: broker_core::Stage3StrategyBarProvenance,
    instrument: InstrumentId,
    start_ts: i64,
    end_ts: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cHistoryWarmupError {
    BrokerTruthExpired,
    LifecycleTimestampReversal,
    EmptyHistory,
    InstrumentMismatch,
    InvalidTimeframe,
    NonFinalBar,
    InvalidOrigin,
    NonMonotonicTimestamp,
    UnalignedTimestamp,
    InvalidOhlc,
    InvalidVolume,
    NoEligibleHistoryBars,
    Stage3ProvenanceRejected,
    FutureHistoryBar,
    InvalidHistoryTimestamp,
}

impl std::fmt::Display for Stage5cHistoryWarmupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Stage 5C history warmup blocked: {self:?}")
    }
}

impl std::error::Error for Stage5cHistoryWarmupError {}

pub struct Stage5cHistoryWarmupReceipt {
    restore_receipt: Stage5cRuntimeStateRestoreReceipt,
    started_ts: DateTime<Utc>,
    processed_bars: usize,
    input_bars: usize,
    source_mode: broker_core::Stage3StrategyBarSourceMode,
    last_history_ts: i64,
}

impl Stage5cHistoryWarmupReceipt {
    pub fn restore_receipt(&self) -> &Stage5cRuntimeStateRestoreReceipt {
        &self.restore_receipt
    }

    pub fn started_ts(&self) -> DateTime<Utc> {
        self.started_ts
    }

    pub fn processed_bars(&self) -> usize {
        self.processed_bars
    }

    pub fn input_bars(&self) -> usize {
        self.input_bars
    }

    pub fn skipped_bars(&self) -> usize {
        self.input_bars.saturating_sub(self.processed_bars)
    }

    pub fn source_mode(&self) -> broker_core::Stage3StrategyBarSourceMode {
        self.source_mode
    }
    pub fn last_history_ts(&self) -> i64 {
        self.last_history_ts
    }

    pub fn warmup_started(&self) -> bool {
        true
    }

    pub fn pending_recovery_started(&self) -> bool {
        false
    }

    pub fn semantic_bar_enabled(&self) -> bool {
        false
    }

    pub fn intent_sink_attached(&self) -> bool {
        false
    }
}

pub struct Stage5cWarmedPaperStrategy {
    strategy: HybridIntradayRuntimeStrategy,
    receipt: Stage5cHistoryWarmupReceipt,
}

impl Stage5cWarmedPaperStrategy {
    pub fn receipt(&self) -> &Stage5cHistoryWarmupReceipt {
        &self.receipt
    }

    #[cfg(test)]
    fn strategy(&self) -> &HybridIntradayRuntimeStrategy {
        &self.strategy
    }

    pub(crate) fn into_parts(self) -> (HybridIntradayRuntimeStrategy, Stage5cHistoryWarmupReceipt) {
        (self.strategy, self.receipt)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stage5cPendingRecoveryPayload {
    Ack(broker_core::HybridRuntimeCommandAck),
    Order(broker_core::HybridRuntimeOrderEvent),
    StopOrder(broker_core::HybridRuntimeStopOrderEvent),
    Position(broker_core::HybridRuntimePositionEvent),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage5cPendingStreamKind {
    Ack,
    Order,
    StopOrder,
    Position,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stage5cPendingRecoveryEvent {
    pub stream_kind: Stage5cPendingStreamKind,
    pub stream_name: String,
    pub entry_id: String,
    pub sequence: u64,
    pub payload: Stage5cPendingRecoveryPayload,
}

pub struct Stage5cPendingStreamClaimBoundary {
    pub stream_kind: Stage5cPendingStreamKind,
    pub stream_name: String,
    pub consumer_group: String,
    pub terminal_claim_cursor: String,
    pub snapshot_boundary_entry_id: String,
    pub claimed_count: usize,
}

pub struct Stage5cPendingRecoveryClaimProofInput {
    pub strategy_id: String,
    pub account_id: BrokerAccountId,
    pub target_instrument: InstrumentId,
    pub snapshot_received_ts: DateTime<Utc>,
    pub completed_ts: DateTime<Utc>,
    pub streams: Vec<Stage5cPendingStreamClaimBoundary>,
}

pub struct Stage5cPendingRecoveryClaimProof {
    strategy_id: String,
    account_id: BrokerAccountId,
    target_instrument: InstrumentId,
    snapshot_received_ts: DateTime<Utc>,
    completed_ts: DateTime<Utc>,
    streams: Vec<Stage5cPendingStreamClaimBoundary>,
}

pub struct Stage5cPendingRecoveryEvidenceInput {
    pub events: Vec<Stage5cPendingRecoveryEvent>,
    pub claim_proof: Stage5cPendingRecoveryClaimProof,
}

pub struct Stage5cAcceptedPendingRecoveryEvidence {
    events: Vec<Stage5cPendingRecoveryEvent>,
    duplicate_events: usize,
    claim_proof: Stage5cPendingRecoveryClaimProof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cPendingRecoveryError {
    EvidenceIncomplete,
    InvalidEventIdentity,
    ConflictingDuplicate,
    NonMonotonicSequence,
    BrokerTruthExpired,
    LifecycleTimestampReversal,
    InstrumentMismatch,
    CallbackValidationFailed,
    UnexpectedIntent,
    ClaimScopeMismatch,
    ClaimBoundaryInvalid,
    StreamKindMismatch,
    FutureEvent,
    InvalidEventTimestamp,
    AckNotPending,
}

impl std::fmt::Display for Stage5cPendingRecoveryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Stage 5C pending recovery blocked: {self:?}")
    }
}

impl std::error::Error for Stage5cPendingRecoveryError {}

pub struct Stage5cPendingRecoveryReceipt {
    warmup_receipt: Stage5cHistoryWarmupReceipt,
    recovered_ts: DateTime<Utc>,
    replayed_events: usize,
    duplicate_events: usize,
}

impl Stage5cPendingRecoveryReceipt {
    pub fn recovered_ts(&self) -> DateTime<Utc> {
        self.recovered_ts
    }
    pub fn replayed_events(&self) -> usize {
        self.replayed_events
    }
    pub fn duplicate_events(&self) -> usize {
        self.duplicate_events
    }
    pub fn pending_recovery_started(&self) -> bool {
        true
    }
    pub fn semantic_bar_enabled(&self) -> bool {
        false
    }
    pub fn intent_sink_attached(&self) -> bool {
        false
    }
    pub fn warmup_receipt(&self) -> &Stage5cHistoryWarmupReceipt {
        &self.warmup_receipt
    }
}

pub struct Stage5cPendingRecoveredPaperStrategy {
    strategy: HybridIntradayRuntimeStrategy,
    receipt: Stage5cPendingRecoveryReceipt,
}

pub struct Stage5cSemanticBarInput {
    pub bar: broker_core::HybridRuntimeBarEvent,
    pub provenance: broker_core::Stage3StrategyBarProvenance,
    pub tick_size: f64,
}

pub struct Stage5cAcceptedSemanticBar {
    bar: broker_core::HybridRuntimeBarEvent,
    tick_size: f64,
    origin: broker_core::HybridRuntimeBarOrigin,
    // STAGE5D-ADDITIVE-BRIDGE-BEGIN: stage5e-b3c-semantic-identity-fields
    #[allow(dead_code)] // Consumed by the closed Stage 5E sequence issuer.
    stage3_provenance_identity: [u8; 32],
    #[allow(dead_code)] // Consumed by the closed Stage 5E sequence issuer.
    semantic_bar_identity: [u8; 32],
    // STAGE5D-ADDITIVE-BRIDGE-END: stage5e-b3c-semantic-identity-fields
}
// STAGE5D-ADDITIVE-BRIDGE-BEGIN: stage5e-b3e-test-corruption-seams
#[cfg(test)]
impl Stage5cAcceptedSemanticBar {
    pub(crate) fn stage5e_test_force_instrument_mismatch(&mut self) {
        self.bar.instrument.symbol.push_str("_MISMATCH");
    }

    pub(crate) fn stage5e_test_force_callback_validation_error(&mut self) {
        self.bar.timeframe_sec = 60;
    }
}
// STAGE5D-ADDITIVE-BRIDGE-END: stage5e-b3e-test-corruption-seams

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cSemanticBarError {
    Stage3Rejected,
    InstrumentMismatch,
    TickSizeMismatch,
    BrokerTruthExpired,
    StaleOrDuplicateBar,
    FutureBar,
    InvalidTimestamp,
    CallbackValidationFailed,
    UnalignedTimestamp,
    InvalidOhlc,
    InvalidVolume,
}

impl std::fmt::Display for Stage5cSemanticBarError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Stage 5C semantic bar blocked: {self:?}")
    }
}
impl std::error::Error for Stage5cSemanticBarError {}

pub struct Stage5cSemanticBarResult {
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    bar_close_ts: i64,
    origin: broker_core::HybridRuntimeBarOrigin,
    execution_eligible: bool,
    intents: Vec<crate::BrokerNeutralHybridIntent>,
    expected_attribution_by_request:
        HashMap<StrategyRequestId, broker_core::HybridRuntimeAttribution>,
}

impl Stage5cSemanticBarResult {
    pub fn bar_close_ts(&self) -> i64 {
        self.bar_close_ts
    }
    pub fn captured_intent_count(&self) -> usize {
        self.intents.len()
    }
    pub fn origin(&self) -> broker_core::HybridRuntimeBarOrigin {
        self.origin
    }
    pub fn execution_eligible(&self) -> bool {
        self.execution_eligible
    }
    pub fn intent_sink_attached(&self) -> bool {
        false
    }
    pub fn broker_transport_attached(&self) -> bool {
        false
    }
    pub fn recovery_receipt(&self) -> &Stage5cPendingRecoveryReceipt {
        &self.recovery_receipt
    }
    pub(crate) fn into_parts(
        self,
    ) -> (
        HybridIntradayRuntimeStrategy,
        Stage5cPendingRecoveryReceipt,
        i64,
        broker_core::HybridRuntimeBarOrigin,
        bool,
        Vec<crate::BrokerNeutralHybridIntent>,
        HashMap<StrategyRequestId, broker_core::HybridRuntimeAttribution>,
    ) {
        (
            self.strategy,
            self.recovery_receipt,
            self.bar_close_ts,
            self.origin,
            self.execution_eligible,
            self.intents,
            self.expected_attribution_by_request,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cIntentSettlementError {
    TooManyIntents,
    MissingIntentClass,
    InstrumentNamespaceMismatch,
    InvalidQuantity,
    InvalidPrice,
    PriceNotTickAligned,
    InvalidStopEnd,
    ReplayIntentNotExecutable,
    MissingPendingRequest,
    RequestIdMismatch,
    DuplicateRequestId,
    UnsupportedIntentAction,
}

impl std::fmt::Display for Stage5cIntentSettlementError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Stage 5C intent settlement blocked: {self:?}")
    }
}
impl std::error::Error for Stage5cIntentSettlementError {}

pub struct Stage5cPaperIntentBatch {
    strategy_id: String,
    account_id: BrokerAccountId,
    instrument: InstrumentId,
    bar_close_ts: i64,
    state_fingerprint: String,
    request_ids: Vec<StrategyRequestId>,
    records: Vec<Stage5cPaperIntentRecord>,
    observation_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5cPaperIntentBatchSummary {
    pub strategy_id: String,
    pub account_id: BrokerAccountId,
    pub instrument: InstrumentId,
    pub origin_bar_close_ts: i64,
    pub bar_close_ts: i64,
    pub min_source_event_ts: i64,
    pub max_source_event_ts: i64,
    pub state_fingerprint: String,
    pub request_ids: Vec<StrategyRequestId>,
    pub intent_count: usize,
    pub observation_only: bool,
}

#[derive(Clone)]
struct Stage5cPaperIntentRecord {
    request_id: StrategyRequestId,
    source_event_ts: i64,
    intent_class: crate::BrokerNeutralHybridIntentClass,
    intent: crate::BrokerNeutralHybridIntent,
    expected_attribution: Option<broker_core::HybridRuntimeAttribution>,
}

// STAGE5G-C-SOURCE-PROJECTION-BEGIN: source-projection-types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage5gSourceBaseAction {
    Market,
    Place,
    Cancel,
    Replace,
    CreateStopLimit,
    DeleteStopLimit,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Stage5gSourceIntentProjection {
    pub request_id: StrategyRequestId,
    pub intent_class: crate::BrokerNeutralHybridIntentClass,
    pub base_action: Stage5gSourceBaseAction,
    pub side: Option<crate::BrokerNeutralOrderSide>,
    pub target_qty: Option<f64>,
    pub pre_position_qty: f64,
    pub expected_attribution: Option<broker_core::HybridRuntimeAttribution>,
}
// STAGE5G-C-SOURCE-PROJECTION-END: source-projection-types

#[derive(Default)]
struct Stage5cCleanupAttributionLedger {
    broker_orders: HashMap<BrokerOrderId, broker_core::HybridRuntimeAttribution>,
    stop_orders: HashMap<BrokerStopOrderId, broker_core::HybridRuntimeAttribution>,
    pending_entry_attribution: Option<broker_core::HybridRuntimeAttribution>,
}

impl Stage5cPaperIntentBatch {
    pub fn intent_count(&self) -> usize {
        self.records.len()
    }
    pub fn request_ids(&self) -> &[StrategyRequestId] {
        &self.request_ids
    }
    pub fn record_request_ids(&self) -> Vec<StrategyRequestId> {
        self.records
            .iter()
            .map(|record| record.request_id)
            .collect()
    }
    pub fn record_source_event_ts_by_request(&self) -> Vec<(StrategyRequestId, i64)> {
        self.records
            .iter()
            .map(|record| (record.request_id, record.source_event_ts))
            .collect()
    }
    pub fn intent_classes(&self) -> Vec<crate::BrokerNeutralHybridIntentClass> {
        self.records
            .iter()
            .map(|record| record.intent_class)
            .collect()
    }
    pub fn has_actionable_intents(&self) -> bool {
        self.records.iter().any(|record| {
            !matches!(
                record.intent.base_intent(),
                crate::BrokerNeutralHybridIntent::Cancel { .. }
            )
        })
    }
    pub fn observation_only(&self) -> bool {
        self.observation_only
    }
    pub fn state_fingerprint(&self) -> &str {
        &self.state_fingerprint
    }
    pub fn bar_close_ts(&self) -> i64 {
        self.bar_close_ts
    }
    pub fn strategy_id(&self) -> &str {
        &self.strategy_id
    }
    pub fn account_id(&self) -> &BrokerAccountId {
        &self.account_id
    }
    pub fn instrument(&self) -> &InstrumentId {
        &self.instrument
    }
}

pub struct Stage5cSettledPaperStrategy {
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    batch: Stage5cPaperIntentBatch,
    settled_batch_history: Vec<Stage5cPaperIntentBatchSummary>,
}

impl std::fmt::Debug for Stage5cSettledPaperStrategy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cSettledPaperStrategy")
            .field("bar_close_ts", &self.batch.bar_close_ts())
            .field("intent_count", &self.batch.intent_count())
            .field(
                "settled_batch_history_len",
                &self.settled_batch_history.len(),
            )
            .field("intent_sink_attached", &false)
            .field("broker_transport_attached", &false)
            .finish_non_exhaustive()
    }
}

impl Stage5cSettledPaperStrategy {
    pub fn intent_batch(&self) -> &Stage5cPaperIntentBatch {
        &self.batch
    }
    pub fn intent_sink_attached(&self) -> bool {
        false
    }
    pub fn broker_transport_attached(&self) -> bool {
        false
    }
    pub fn recovery_receipt(&self) -> &Stage5cPendingRecoveryReceipt {
        &self.recovery_receipt
    }
    pub fn settled_batch_history(&self) -> &[Stage5cPaperIntentBatchSummary] {
        &self.settled_batch_history
    }
    pub fn timer_path_enabled(&self) -> bool {
        false
    }
    // STAGE5G-C-SOURCE-PROJECTION-BEGIN: settled-test-read-only-accessor
    #[cfg(test)]
    pub(crate) fn stage5g_source_intent_projections(&self) -> Vec<Stage5gSourceIntentProjection> {
        stage5g_source_intent_projections(&self.strategy, &self.batch)
    }
    // STAGE5G-C-SOURCE-PROJECTION-END: settled-test-read-only-accessor
    #[cfg(test)]
    fn strategy(&self) -> &HybridIntradayRuntimeStrategy {
        &self.strategy
    }
    pub(crate) fn into_parts(
        self,
    ) -> (
        HybridIntradayRuntimeStrategy,
        Stage5cPendingRecoveryReceipt,
        Stage5cPaperIntentBatch,
    ) {
        (self.strategy, self.recovery_receipt, self.batch)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cNextBarLoopError {
    NonMonotonicBar,
    UnresolvedIntentBatch,
    Semantic(Stage5cSemanticBarError),
    Settlement(Stage5cIntentSettlementError),
}

impl std::fmt::Display for Stage5cNextBarLoopError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Stage 5C controlled next-bar loop blocked: {self:?}"
        )
    }
}

impl std::error::Error for Stage5cNextBarLoopError {}

pub struct Stage5cNextBarBlocked {
    reason: Stage5cNextBarLoopError,
    settled: Stage5cSettledPaperStrategy,
}

impl Stage5cNextBarBlocked {
    pub fn reason(&self) -> Stage5cNextBarLoopError {
        self.reason
    }
    pub fn settled(&self) -> &Stage5cSettledPaperStrategy {
        &self.settled
    }
    pub fn into_settled(self) -> Stage5cSettledPaperStrategy {
        self.settled
    }
}

impl std::fmt::Debug for Stage5cNextBarBlocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cNextBarBlocked")
            .field("reason", &self.reason)
            .field(
                "previous_bar_close_ts",
                &self.settled.intent_batch().bar_close_ts(),
            )
            .field(
                "previous_intent_count",
                &self.settled.intent_batch().intent_count(),
            )
            .finish_non_exhaustive()
    }
}

impl std::fmt::Display for Stage5cNextBarBlocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Stage 5C next bar blocked: {:?}", self.reason)
    }
}

impl std::error::Error for Stage5cNextBarBlocked {}

#[derive(Debug)]
pub enum Stage5cNextBarLoopFailure {
    Blocked(Box<Stage5cNextBarBlocked>),
    Failed(Stage5cNextBarLoopError),
}

impl Stage5cNextBarLoopFailure {
    pub fn reason(&self) -> Stage5cNextBarLoopError {
        match self {
            Self::Blocked(blocked) => blocked.reason(),
            Self::Failed(reason) => *reason,
        }
    }
    pub fn into_blocked(self) -> Option<Stage5cNextBarBlocked> {
        match self {
            Self::Blocked(blocked) => Some(*blocked),
            Self::Failed(_) => None,
        }
    }
}

impl std::fmt::Display for Stage5cNextBarLoopFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Stage 5C next bar failed: {:?}", self.reason())
    }
}

impl std::error::Error for Stage5cNextBarLoopFailure {}

#[derive(Debug, Clone)]
pub struct Stage5cPaperIntentLifecycleInput {
    pub ack_records: Vec<Stage5cPaperAckRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5cPaperAckRecord {
    pub total_sequence: u64,
    pub ack: broker_core::HybridRuntimeCommandAck,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5cPaperAckOutcome {
    pub total_sequence: u64,
    pub request_id: StrategyRequestId,
    pub status: broker_core::HybridRuntimeAckStatus,
    pub broker_order_id: Option<BrokerOrderId>,
    pub error_code: Option<broker_core::HybridRuntimeAckErrorCode>,
    pub processed_ts_utc: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cPaperBrokerEventKind {
    Order,
    StopOrder,
    Position,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Stage5cPaperBrokerEventPayload {
    Order(broker_core::HybridRuntimeOrderEvent),
    StopOrder(broker_core::HybridRuntimeStopOrderEvent),
    Position(broker_core::HybridRuntimePositionEvent),
}

impl Stage5cPaperBrokerEventPayload {
    fn kind(&self) -> Stage5cPaperBrokerEventKind {
        match self {
            Self::Order(_) => Stage5cPaperBrokerEventKind::Order,
            Self::StopOrder(_) => Stage5cPaperBrokerEventKind::StopOrder,
            Self::Position(_) => Stage5cPaperBrokerEventKind::Position,
        }
    }
    fn instrument(&self) -> &InstrumentId {
        match self {
            Self::Order(value) => &value.instrument,
            Self::StopOrder(value) => &value.instrument,
            Self::Position(value) => &value.instrument,
        }
    }
    fn source_ts_utc(&self) -> i64 {
        match self {
            Self::Order(value) => value.source_ts_utc,
            Self::StopOrder(value) => value.source_ts_utc,
            Self::Position(value) => value.source_ts_utc,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage5cPaperBrokerEventRecord {
    pub total_sequence: u64,
    pub request_id: StrategyRequestId,
    pub payload: Stage5cPaperBrokerEventPayload,
}

#[derive(Debug, Clone)]
pub struct Stage5cPaperBrokerLifecycleInput {
    pub event_records: Vec<Stage5cPaperBrokerEventRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cPaperIntentLifecycleError {
    EmptyIntentBatch,
    StateFingerprintMismatch,
    MissingAck,
    DuplicateAck,
    UnknownAckRequestId,
    DuplicateSequence,
    NonMonotonicSequence,
    AckTimestampBeforeIntentBar,
    CallbackValidationFailed,
    CallbackGeneratedIntentTerminal,
}

impl std::fmt::Display for Stage5cPaperIntentLifecycleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Stage 5C paper intent lifecycle blocked: {self:?}"
        )
    }
}

impl std::error::Error for Stage5cPaperIntentLifecycleError {}

pub struct Stage5cPaperIntentLifecycleBlocked {
    reason: Stage5cPaperIntentLifecycleError,
    settled: Stage5cSettledPaperStrategy,
}

impl Stage5cPaperIntentLifecycleBlocked {
    pub fn reason(&self) -> Stage5cPaperIntentLifecycleError {
        self.reason
    }
    pub fn settled(&self) -> &Stage5cSettledPaperStrategy {
        &self.settled
    }
    pub fn into_settled(self) -> Stage5cSettledPaperStrategy {
        self.settled
    }
}

impl std::fmt::Debug for Stage5cPaperIntentLifecycleBlocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cPaperIntentLifecycleBlocked")
            .field("reason", &self.reason)
            .field("bar_close_ts", &self.settled.intent_batch().bar_close_ts())
            .field("intent_count", &self.settled.intent_batch().intent_count())
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum Stage5cPaperIntentLifecycleFailure {
    Blocked(Box<Stage5cPaperIntentLifecycleBlocked>),
    Terminal(Stage5cPaperIntentLifecycleError),
}

impl Stage5cPaperIntentLifecycleFailure {
    pub fn reason(&self) -> Stage5cPaperIntentLifecycleError {
        match self {
            Self::Blocked(blocked) => blocked.reason(),
            Self::Terminal(reason) => *reason,
        }
    }
    pub fn into_blocked(self) -> Option<Stage5cPaperIntentLifecycleBlocked> {
        match self {
            Self::Blocked(blocked) => Some(*blocked),
            Self::Terminal(_) => None,
        }
    }
}

impl std::fmt::Display for Stage5cPaperIntentLifecycleFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Stage 5C paper intent lifecycle failed: {:?}",
            self.reason()
        )
    }
}

impl std::error::Error for Stage5cPaperIntentLifecycleFailure {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cPaperBrokerLifecycleError {
    DuplicateSequence,
    UnknownEventRequestId,
    DuplicateEvent,
    ConflictingDuplicateEvent,
    EventForTerminalAck,
    MissingExpectedBrokerEvent,
    UnexpectedBrokerEventKind,
    EventTimestampBeforeAck,
    InstrumentMismatch,
    OrderRequestIdMismatch,
    BrokerOrderIdMismatch,
    StopOrderIdMismatch,
    PositionEventRequiresMarketIntent,
    PositionSideMismatch,
    PositionOverfill,
    PositionRegression,
    AttributionMissing,
    AttributionStrategyMismatch,
    AttributionRoleMismatch,
    AttributionCycleMismatch,
    IntentFieldMismatch,
    MissingTerminalLifecycleEvent,
    UnknownOrderStatus,
    UnknownStopOrderStatus,
    CallbackValidationFailed,
    CallbackGeneratedIntentTerminal,
}

impl std::fmt::Display for Stage5cPaperBrokerLifecycleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Stage 5C paper broker lifecycle blocked: {self:?}"
        )
    }
}

impl std::error::Error for Stage5cPaperBrokerLifecycleError {}

pub struct Stage5cPaperBrokerLifecycleBlocked {
    reason: Stage5cPaperBrokerLifecycleError,
    resolved: Stage5cResolvedPaperIntentBatchStrategy,
}

impl Stage5cPaperBrokerLifecycleBlocked {
    pub fn reason(&self) -> Stage5cPaperBrokerLifecycleError {
        self.reason
    }
    pub fn resolved(&self) -> &Stage5cResolvedPaperIntentBatchStrategy {
        &self.resolved
    }
    pub fn into_resolved(self) -> Stage5cResolvedPaperIntentBatchStrategy {
        self.resolved
    }
}

impl std::fmt::Debug for Stage5cPaperBrokerLifecycleBlocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cPaperBrokerLifecycleBlocked")
            .field("reason", &self.reason)
            .field(
                "resolved_bar_close_ts",
                &self.resolved.resolved_batch.bar_close_ts(),
            )
            .field(
                "resolved_intent_count",
                &self.resolved.resolved_batch.intent_count(),
            )
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum Stage5cPaperBrokerLifecycleFailure {
    Blocked(Box<Stage5cPaperBrokerLifecycleBlocked>),
    Terminal(Stage5cPaperBrokerLifecycleError),
}

impl Stage5cPaperBrokerLifecycleFailure {
    pub fn reason(&self) -> Stage5cPaperBrokerLifecycleError {
        match self {
            Self::Blocked(blocked) => blocked.reason(),
            Self::Terminal(reason) => *reason,
        }
    }
    pub fn into_blocked(self) -> Option<Stage5cPaperBrokerLifecycleBlocked> {
        match self {
            Self::Blocked(blocked) => Some(*blocked),
            Self::Terminal(_) => None,
        }
    }
}

impl std::fmt::Display for Stage5cPaperBrokerLifecycleFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Stage 5C paper broker lifecycle failed: {:?}",
            self.reason()
        )
    }
}

impl std::error::Error for Stage5cPaperBrokerLifecycleFailure {}

pub struct Stage5cResolvedPaperIntentBatchStrategy {
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    resolved_batch: Stage5cPaperIntentBatch,
    ack_outcomes: Vec<Stage5cPaperAckOutcome>,
    settled_batch_history: Vec<Stage5cPaperIntentBatchSummary>,
}

impl std::fmt::Debug for Stage5cResolvedPaperIntentBatchStrategy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cResolvedPaperIntentBatchStrategy")
            .field("resolved_bar_close_ts", &self.resolved_batch.bar_close_ts())
            .field("resolved_intent_count", &self.resolved_batch.intent_count())
            .field("ack_outcome_count", &self.ack_outcomes.len())
            .field(
                "settled_batch_history_len",
                &self.settled_batch_history.len(),
            )
            .field("intent_sink_attached", &false)
            .field("broker_transport_attached", &false)
            .finish_non_exhaustive()
    }
}

impl Stage5cResolvedPaperIntentBatchStrategy {
    pub fn resolved_batch_summary(&self) -> Stage5cPaperIntentBatchSummary {
        stage5ch_batch_summary(&self.resolved_batch)
    }
    pub fn ack_outcomes(&self) -> &[Stage5cPaperAckOutcome] {
        &self.ack_outcomes
    }
    #[cfg(test)]
    fn full_resolved_batch(&self) -> &Stage5cPaperIntentBatch {
        &self.resolved_batch
    }
    pub fn settled_batch_history(&self) -> &[Stage5cPaperIntentBatchSummary] {
        &self.settled_batch_history
    }
    pub fn intent_sink_attached(&self) -> bool {
        false
    }
    pub fn broker_transport_attached(&self) -> bool {
        false
    }
    pub fn timer_path_enabled(&self) -> bool {
        false
    }
    pub fn recovery_receipt(&self) -> &Stage5cPendingRecoveryReceipt {
        &self.recovery_receipt
    }
    pub fn post_lifecycle_state_fingerprint(&self) -> String {
        stage5c_state_fingerprint(Strategy::state(&self.strategy))
    }
    // STAGE5G-C-SOURCE-PROJECTION-BEGIN: resolved-read-only-accessor
    pub(crate) fn stage5g_source_intent_projections(&self) -> Vec<Stage5gSourceIntentProjection> {
        stage5g_source_intent_projections(&self.strategy, &self.resolved_batch)
    }
    // STAGE5G-C-SOURCE-PROJECTION-END: resolved-read-only-accessor
    #[cfg(test)]
    fn strategy(&self) -> &HybridIntradayRuntimeStrategy {
        &self.strategy
    }
}

// STAGE5G-C-SOURCE-PROJECTION-BEGIN: source-projection-function
fn stage5g_source_intent_projections(
    strategy: &HybridIntradayRuntimeStrategy,
    batch: &Stage5cPaperIntentBatch,
) -> Vec<Stage5gSourceIntentProjection> {
    let pre_position_qty = stage5cj_position_qty(Strategy::state(strategy));
    batch
        .records
        .iter()
        .map(|record| {
            let (base_action, side, target_qty) = match record.intent.base_intent() {
                crate::BrokerNeutralHybridIntent::Market { side, qty, .. } => {
                    (Stage5gSourceBaseAction::Market, Some(*side), Some(*qty))
                }
                crate::BrokerNeutralHybridIntent::Place { side, qty, .. } => {
                    (Stage5gSourceBaseAction::Place, Some(*side), Some(*qty))
                }
                crate::BrokerNeutralHybridIntent::Cancel { .. } => {
                    (Stage5gSourceBaseAction::Cancel, None, None)
                }
                crate::BrokerNeutralHybridIntent::Replace { new_qty, .. } => {
                    (Stage5gSourceBaseAction::Replace, None, Some(*new_qty))
                }
                crate::BrokerNeutralHybridIntent::CreateStopLimit { side, qty, .. } => (
                    Stage5gSourceBaseAction::CreateStopLimit,
                    Some(*side),
                    Some(*qty),
                ),
                crate::BrokerNeutralHybridIntent::DeleteStopLimit { side, .. } => {
                    (Stage5gSourceBaseAction::DeleteStopLimit, *side, None)
                }
                crate::BrokerNeutralHybridIntent::Classified { .. }
                | crate::BrokerNeutralHybridIntent::Routed { .. } => {
                    unreachable!("base_intent unwraps wrappers")
                }
            };
            Stage5gSourceIntentProjection {
                request_id: record.request_id,
                intent_class: record.intent_class,
                base_action,
                side,
                target_qty,
                pre_position_qty,
                expected_attribution: record.expected_attribution.clone(),
            }
        })
        .collect()
}
// STAGE5G-C-SOURCE-PROJECTION-END: source-projection-function

pub struct Stage5cBrokerLifecycleResolvedPaperStrategy {
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    resolved_batch: Stage5cPaperIntentBatch,
    resolved_batch_summary: Stage5cPaperIntentBatchSummary,
    ack_outcomes: Vec<Stage5cPaperAckOutcome>,
    broker_event_count: usize,
    remaining_lifecycle_expectations: Vec<Stage5cPaperBrokerLifecycleExpectation>,
    lifecycle_watermark_ts_utc: i64,
    generated_intent_batch: Option<Stage5cPaperIntentBatch>,
    settled_batch_history: Vec<Stage5cPaperIntentBatchSummary>,
}

pub struct Stage5cBrokerLifecycleSettlement {
    inner: Stage5cBrokerLifecycleSettlementKind,
}

enum Stage5cBrokerLifecycleSettlementKind {
    ReadyForTimer(Stage5cBrokerLifecycleResolvedPaperStrategy),
    GeneratedIntentBatch(Stage5cSettledPaperStrategy),
    UnresolvedBrokerLifecycle(Stage5cBrokerLifecycleResolvedPaperStrategy),
}

impl std::fmt::Debug for Stage5cBrokerLifecycleSettlement {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (kind, intent_count, remaining_expectation_count) = match &self.inner {
            Stage5cBrokerLifecycleSettlementKind::ReadyForTimer(resolved) => (
                "ReadyForTimer",
                resolved.generated_intent_count(),
                resolved.remaining_lifecycle_expectations().len(),
            ),
            Stage5cBrokerLifecycleSettlementKind::GeneratedIntentBatch(settled) => (
                "GeneratedIntentBatch",
                settled.intent_batch().intent_count(),
                0,
            ),
            Stage5cBrokerLifecycleSettlementKind::UnresolvedBrokerLifecycle(resolved) => (
                "UnresolvedBrokerLifecycle",
                resolved.generated_intent_count(),
                resolved.remaining_lifecycle_expectations().len(),
            ),
        };
        formatter
            .debug_struct("Stage5cBrokerLifecycleSettlement")
            .field("kind", &kind)
            .field("intent_count", &intent_count)
            .field(
                "remaining_lifecycle_expectation_count",
                &remaining_expectation_count,
            )
            .field("intent_sink_attached", &false)
            .field("broker_transport_attached", &false)
            .finish_non_exhaustive()
    }
}

impl Stage5cBrokerLifecycleSettlement {
    fn ready_for_timer(resolved: Stage5cBrokerLifecycleResolvedPaperStrategy) -> Self {
        Self {
            inner: Stage5cBrokerLifecycleSettlementKind::ReadyForTimer(resolved),
        }
    }

    fn generated_intent_batch(settled: Stage5cSettledPaperStrategy) -> Self {
        Self {
            inner: Stage5cBrokerLifecycleSettlementKind::GeneratedIntentBatch(settled),
        }
    }

    fn unresolved_broker_lifecycle(resolved: Stage5cBrokerLifecycleResolvedPaperStrategy) -> Self {
        Self {
            inner: Stage5cBrokerLifecycleSettlementKind::UnresolvedBrokerLifecycle(resolved),
        }
    }

    pub fn is_ready_for_timer(&self) -> bool {
        matches!(
            self.inner,
            Stage5cBrokerLifecycleSettlementKind::ReadyForTimer(_)
        )
    }

    pub fn is_generated_intent_batch(&self) -> bool {
        matches!(
            self.inner,
            Stage5cBrokerLifecycleSettlementKind::GeneratedIntentBatch(_)
        )
    }

    pub fn is_unresolved_broker_lifecycle(&self) -> bool {
        matches!(
            self.inner,
            Stage5cBrokerLifecycleSettlementKind::UnresolvedBrokerLifecycle(_)
        )
    }

    pub fn intent_sink_attached(&self) -> bool {
        false
    }

    pub fn broker_transport_attached(&self) -> bool {
        false
    }

    pub fn redis_command_stream_attached(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cPaperTimerError {
    BrokerTruthExpired,
    NonMonotonicTimer,
    UnresolvedBrokerLifecycle,
    UnresolvedGeneratedIntentBatch,
    CallbackValidationFailed,
    GeneratedIntentTerminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5cPaperTimerInput {
    pub now_ts_utc_ms: i64,
}

pub struct Stage5cTimerResolvedPaperStrategy {
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    resolved_batch_summary: Stage5cPaperIntentBatchSummary,
    timer_ts_utc_ms: i64,
    generated_intent_batch: Option<Stage5cPaperIntentBatch>,
    settled_batch_history: Vec<Stage5cPaperIntentBatchSummary>,
}

impl std::fmt::Debug for Stage5cTimerResolvedPaperStrategy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cTimerResolvedPaperStrategy")
            .field("timer_ts_utc_ms", &self.timer_ts_utc_ms)
            .field("generated_intent_count", &self.generated_intent_count())
            .field("intent_sink_attached", &false)
            .field("broker_transport_attached", &false)
            .finish_non_exhaustive()
    }
}

impl Stage5cTimerResolvedPaperStrategy {
    pub fn timer_ts_utc_ms(&self) -> i64 {
        self.timer_ts_utc_ms
    }
    pub fn generated_intent_count(&self) -> usize {
        self.generated_intent_batch
            .as_ref()
            .map(Stage5cPaperIntentBatch::intent_count)
            .unwrap_or_default()
    }
    pub fn generated_intent_batch_summary(&self) -> Option<Stage5cPaperIntentBatchSummary> {
        self.generated_intent_batch
            .as_ref()
            .map(stage5ch_batch_summary)
    }
    pub fn settled_batch_history(&self) -> &[Stage5cPaperIntentBatchSummary] {
        &self.settled_batch_history
    }
    pub fn recovery_receipt(&self) -> &Stage5cPendingRecoveryReceipt {
        &self.recovery_receipt
    }
    pub fn resolved_batch_summary(&self) -> &Stage5cPaperIntentBatchSummary {
        &self.resolved_batch_summary
    }
    pub fn post_timer_state_fingerprint(&self) -> String {
        stage5c_state_fingerprint(Strategy::state(&self.strategy))
    }
    pub fn intent_sink_attached(&self) -> bool {
        false
    }
    pub fn broker_transport_attached(&self) -> bool {
        false
    }
    pub fn redis_command_stream_attached(&self) -> bool {
        false
    }
}

pub struct Stage5cTimerSettlement {
    inner: Stage5cTimerSettlementKind,
}

enum Stage5cTimerSettlementKind {
    ReadyForContinuation {
        settled: Stage5cSettledPaperStrategy,
        checkpoint_ts_utc_ms: i64,
    },
    GeneratedIntentBatch(Stage5cSettledPaperStrategy),
}

impl std::fmt::Debug for Stage5cTimerSettlement {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (kind, settled, checkpoint_ts_utc_ms) = match &self.inner {
            Stage5cTimerSettlementKind::ReadyForContinuation {
                settled,
                checkpoint_ts_utc_ms,
            } => ("ReadyForContinuation", settled, Some(*checkpoint_ts_utc_ms)),
            Stage5cTimerSettlementKind::GeneratedIntentBatch(settled) => {
                ("GeneratedIntentBatch", settled, None)
            }
        };
        formatter
            .debug_struct("Stage5cTimerSettlement")
            .field("kind", &kind)
            .field("checkpoint_ts_utc_ms", &checkpoint_ts_utc_ms)
            .field(
                "timer_result_intent_count",
                &settled.intent_batch().intent_count(),
            )
            .field("intent_sink_attached", &false)
            .field("broker_transport_attached", &false)
            .finish_non_exhaustive()
    }
}

impl Stage5cTimerSettlement {
    fn ready_for_continuation(
        settled: Stage5cSettledPaperStrategy,
        checkpoint_ts_utc_ms: i64,
    ) -> Self {
        Self {
            inner: Stage5cTimerSettlementKind::ReadyForContinuation {
                settled,
                checkpoint_ts_utc_ms,
            },
        }
    }

    fn generated_intent_batch(settled: Stage5cSettledPaperStrategy) -> Self {
        Self {
            inner: Stage5cTimerSettlementKind::GeneratedIntentBatch(settled),
        }
    }

    pub fn is_ready_for_continuation(&self) -> bool {
        matches!(
            self.inner,
            Stage5cTimerSettlementKind::ReadyForContinuation { .. }
        )
    }
    pub fn is_generated_intent_batch(&self) -> bool {
        matches!(
            self.inner,
            Stage5cTimerSettlementKind::GeneratedIntentBatch(_)
        )
    }
    pub fn settled(&self) -> &Stage5cSettledPaperStrategy {
        match &self.inner {
            Stage5cTimerSettlementKind::ReadyForContinuation { settled, .. }
            | Stage5cTimerSettlementKind::GeneratedIntentBatch(settled) => settled,
        }
    }
    pub fn into_generated_intent_batch(self) -> Result<Stage5cSettledPaperStrategy, Box<Self>> {
        match self.inner {
            Stage5cTimerSettlementKind::GeneratedIntentBatch(settled) => Ok(settled),
            Stage5cTimerSettlementKind::ReadyForContinuation {
                settled,
                checkpoint_ts_utc_ms,
            } => Err(Box::new(Self::ready_for_continuation(
                settled,
                checkpoint_ts_utc_ms,
            ))),
        }
    }
    pub fn checkpoint_ts_utc_ms(&self) -> Option<i64> {
        match &self.inner {
            Stage5cTimerSettlementKind::ReadyForContinuation {
                checkpoint_ts_utc_ms,
                ..
            } => Some(*checkpoint_ts_utc_ms),
            Stage5cTimerSettlementKind::GeneratedIntentBatch(_) => None,
        }
    }
    pub fn intent_sink_attached(&self) -> bool {
        false
    }
    pub fn broker_transport_attached(&self) -> bool {
        false
    }
    pub fn redis_command_stream_attached(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cTimerContinuationError {
    GeneratedIntentBatchRequiresLifecycle,
    NonMonotonicTimer,
    BrokerTruthExpired,
    CallbackValidationFailed,
    GeneratedIntentTerminal,
    NextBar(Stage5cNextBarLoopError),
}

pub struct Stage5cTimerContinuationBlocked {
    reason: Stage5cTimerContinuationError,
    settlement: Stage5cTimerSettlement,
}

impl Stage5cTimerContinuationBlocked {
    pub fn reason(&self) -> Stage5cTimerContinuationError {
        self.reason
    }
    pub fn settlement(&self) -> &Stage5cTimerSettlement {
        &self.settlement
    }
    pub fn into_settlement(self) -> Stage5cTimerSettlement {
        self.settlement
    }
}

impl std::fmt::Debug for Stage5cTimerContinuationBlocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cTimerContinuationBlocked")
            .field("reason", &self.reason)
            .field(
                "checkpoint_ts_utc_ms",
                &self.settlement.checkpoint_ts_utc_ms(),
            )
            .field(
                "intent_count",
                &self.settlement.settled().intent_batch().intent_count(),
            )
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum Stage5cTimerContinuationFailure {
    Blocked(Box<Stage5cTimerContinuationBlocked>),
    Terminal(Stage5cTimerContinuationError),
}

impl Stage5cTimerContinuationFailure {
    pub fn reason(&self) -> Stage5cTimerContinuationError {
        match self {
            Self::Blocked(blocked) => blocked.reason(),
            Self::Terminal(reason) => *reason,
        }
    }
    pub fn into_blocked(self) -> Option<Stage5cTimerContinuationBlocked> {
        match self {
            Self::Blocked(blocked) => Some(*blocked),
            Self::Terminal(_) => None,
        }
    }
}

impl std::fmt::Display for Stage5cTimerContinuationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Stage 5C timer continuation failed: {:?}",
            self.reason()
        )
    }
}

impl std::error::Error for Stage5cTimerContinuationFailure {}

pub struct Stage5cPaperTimerBlocked {
    reason: Stage5cPaperTimerError,
    resolved: Stage5cBrokerLifecycleResolvedPaperStrategy,
}

impl std::fmt::Debug for Stage5cPaperTimerBlocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cPaperTimerBlocked")
            .field("reason", &self.reason)
            .field(
                "resolved_bar_close_ts",
                &self.resolved.resolved_batch_summary.bar_close_ts,
            )
            .finish_non_exhaustive()
    }
}

impl Stage5cPaperTimerBlocked {
    pub fn reason(&self) -> Stage5cPaperTimerError {
        self.reason
    }
    pub fn resolved(&self) -> &Stage5cBrokerLifecycleResolvedPaperStrategy {
        &self.resolved
    }
    fn into_resolved(self) -> Stage5cBrokerLifecycleResolvedPaperStrategy {
        self.resolved
    }
}

#[derive(Debug)]
pub enum Stage5cPaperTimerFailure {
    Blocked(Box<Stage5cPaperTimerBlocked>),
    Terminal(Stage5cPaperTimerError),
}

impl Stage5cPaperTimerFailure {
    pub fn reason(&self) -> Stage5cPaperTimerError {
        match self {
            Self::Blocked(blocked) => blocked.reason(),
            Self::Terminal(reason) => *reason,
        }
    }
    fn into_blocked(self) -> Option<Box<Stage5cPaperTimerBlocked>> {
        match self {
            Self::Blocked(blocked) => Some(blocked),
            Self::Terminal(_) => None,
        }
    }
}

impl std::fmt::Display for Stage5cPaperTimerFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Stage 5C paper timer failed: {:?}",
            self.reason()
        )
    }
}

impl std::error::Error for Stage5cPaperTimerFailure {}

impl std::fmt::Debug for Stage5cBrokerLifecycleResolvedPaperStrategy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cBrokerLifecycleResolvedPaperStrategy")
            .field(
                "resolved_bar_close_ts",
                &self.resolved_batch_summary.bar_close_ts,
            )
            .field("broker_event_count", &self.broker_event_count)
            .field(
                "lifecycle_watermark_ts_utc",
                &self.lifecycle_watermark_ts_utc,
            )
            .field(
                "remaining_lifecycle_expectation_count",
                &self.remaining_lifecycle_expectations.len(),
            )
            .field(
                "generated_intent_count",
                &self
                    .generated_intent_batch
                    .as_ref()
                    .map(Stage5cPaperIntentBatch::intent_count)
                    .unwrap_or_default(),
            )
            .field("intent_sink_attached", &false)
            .field("broker_transport_attached", &false)
            .finish_non_exhaustive()
    }
}

impl Stage5cBrokerLifecycleResolvedPaperStrategy {
    pub fn resolved_batch_summary(&self) -> &Stage5cPaperIntentBatchSummary {
        &self.resolved_batch_summary
    }
    pub fn full_resolved_intent_count(&self) -> usize {
        self.resolved_batch.intent_count()
    }
    #[cfg(test)]
    fn full_resolved_batch(&self) -> &Stage5cPaperIntentBatch {
        &self.resolved_batch
    }
    pub fn ack_outcomes(&self) -> &[Stage5cPaperAckOutcome] {
        &self.ack_outcomes
    }
    pub fn broker_event_count(&self) -> usize {
        self.broker_event_count
    }
    pub fn remaining_lifecycle_expectations(&self) -> &[Stage5cPaperBrokerLifecycleExpectation] {
        &self.remaining_lifecycle_expectations
    }
    pub fn lifecycle_watermark_ts_utc(&self) -> i64 {
        self.lifecycle_watermark_ts_utc
    }
    pub fn generated_intent_count(&self) -> usize {
        self.generated_intent_batch
            .as_ref()
            .map(Stage5cPaperIntentBatch::intent_count)
            .unwrap_or_default()
    }
    pub fn generated_intent_batch_summary(&self) -> Option<Stage5cPaperIntentBatchSummary> {
        self.generated_intent_batch
            .as_ref()
            .map(stage5ch_batch_summary)
    }
    #[cfg(test)]
    fn generated_intent_batch(&self) -> Option<&Stage5cPaperIntentBatch> {
        self.generated_intent_batch.as_ref()
    }
    pub fn settled_batch_history(&self) -> &[Stage5cPaperIntentBatchSummary] {
        &self.settled_batch_history
    }
    pub fn intent_sink_attached(&self) -> bool {
        false
    }
    pub fn broker_transport_attached(&self) -> bool {
        false
    }
    pub fn timer_path_enabled(&self) -> bool {
        false
    }
    pub fn recovery_receipt(&self) -> &Stage5cPendingRecoveryReceipt {
        &self.recovery_receipt
    }
    pub fn post_broker_lifecycle_state_fingerprint(&self) -> String {
        stage5c_state_fingerprint(Strategy::state(&self.strategy))
    }
    #[cfg(test)]
    fn strategy(&self) -> &HybridIntradayRuntimeStrategy {
        &self.strategy
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5cPaperBrokerLifecycleExpectation {
    pub request_id: StrategyRequestId,
    pub expected_event_kind: Stage5cPaperBrokerEventKind,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cPaperLoopStateKind {
    PendingRecovered,
    SemanticResult,
    Settled,
    IntentLifecycleResolved,
    BrokerLifecycleResolved,
    BrokerLifecycleSettlement,
    TimerResolved,
    TimerSettlement,
}

pub enum Stage5cPaperLoopState {
    PendingRecovered(Box<Stage5cPendingRecoveredPaperStrategy>),
    SemanticResult(Box<Stage5cSemanticBarResult>),
    Settled(Box<Stage5cSettledPaperStrategy>),
    IntentLifecycleResolved(Box<Stage5cResolvedPaperIntentBatchStrategy>),
    BrokerLifecycleResolved(Box<Stage5cBrokerLifecycleResolvedPaperStrategy>),
    BrokerLifecycleSettlement(Box<Stage5cBrokerLifecycleSettlement>),
    TimerResolved(Box<Stage5cTimerResolvedPaperStrategy>),
    TimerSettlement(Box<Stage5cTimerSettlement>),
}

impl std::fmt::Debug for Stage5cPaperLoopState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cPaperLoopState")
            .field("kind", &self.kind())
            .field("intent_sink_attached", &self.intent_sink_attached())
            .field(
                "broker_transport_attached",
                &self.broker_transport_attached(),
            )
            .field(
                "redis_command_stream_attached",
                &self.redis_command_stream_attached(),
            )
            .finish_non_exhaustive()
    }
}

impl Stage5cPaperLoopState {
    pub fn kind(&self) -> Stage5cPaperLoopStateKind {
        match self {
            Self::PendingRecovered(_) => Stage5cPaperLoopStateKind::PendingRecovered,
            Self::SemanticResult(_) => Stage5cPaperLoopStateKind::SemanticResult,
            Self::Settled(_) => Stage5cPaperLoopStateKind::Settled,
            Self::IntentLifecycleResolved(_) => Stage5cPaperLoopStateKind::IntentLifecycleResolved,
            Self::BrokerLifecycleResolved(_) => Stage5cPaperLoopStateKind::BrokerLifecycleResolved,
            Self::BrokerLifecycleSettlement(_) => {
                Stage5cPaperLoopStateKind::BrokerLifecycleSettlement
            }
            Self::TimerResolved(_) => Stage5cPaperLoopStateKind::TimerResolved,
            Self::TimerSettlement(_) => Stage5cPaperLoopStateKind::TimerSettlement,
        }
    }

    pub fn intent_sink_attached(&self) -> bool {
        false
    }

    pub fn broker_transport_attached(&self) -> bool {
        false
    }

    pub fn redis_command_stream_attached(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cPaperLoopEventKind {
    FinalM10Bar,
    SettleSemanticResult,
    Ack,
    BrokerLifecycleBatch,
    SettleBrokerLifecycleResult,
    Timer,
    SettleTimerResult,
}

pub enum Stage5cPaperLoopEvent {
    FinalM10Bar(Box<Stage5cAcceptedSemanticBar>),
    SettleSemanticResult,
    Ack(Box<Stage5cPaperIntentLifecycleInput>),
    BrokerLifecycleBatch(Box<Stage5cPaperBrokerLifecycleInput>),
    SettleBrokerLifecycleResult,
    Timer(Stage5cPaperTimerInput),
    SettleTimerResult,
}

impl std::fmt::Debug for Stage5cPaperLoopEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cPaperLoopEvent")
            .field("kind", &self.kind())
            .finish_non_exhaustive()
    }
}

impl Stage5cPaperLoopEvent {
    pub fn kind(&self) -> Stage5cPaperLoopEventKind {
        match self {
            Self::FinalM10Bar(_) => Stage5cPaperLoopEventKind::FinalM10Bar,
            Self::SettleSemanticResult => Stage5cPaperLoopEventKind::SettleSemanticResult,
            Self::Ack(_) => Stage5cPaperLoopEventKind::Ack,
            Self::BrokerLifecycleBatch(_) => Stage5cPaperLoopEventKind::BrokerLifecycleBatch,
            Self::SettleBrokerLifecycleResult => {
                Stage5cPaperLoopEventKind::SettleBrokerLifecycleResult
            }
            Self::Timer(_) => Stage5cPaperLoopEventKind::Timer,
            Self::SettleTimerResult => Stage5cPaperLoopEventKind::SettleTimerResult,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cPaperLoopError {
    InvalidTransition {
        state: Stage5cPaperLoopStateKind,
        event: Stage5cPaperLoopEventKind,
    },
    Semantic(Stage5cSemanticBarError),
    IntentSettlement(Stage5cIntentSettlementError),
    NextBar(Stage5cNextBarLoopError),
    IntentLifecycle(Stage5cPaperIntentLifecycleError),
    BrokerLifecycle(Stage5cPaperBrokerLifecycleError),
    BrokerLifecycleIncompleteBatch,
    Timer(Stage5cPaperTimerError),
    BrokerLifecycleRequiresGeneratedAck,
    BrokerLifecycleUnresolved,
    TimerContinuation(Stage5cTimerContinuationError),
}

impl std::fmt::Display for Stage5cPaperLoopError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Stage 5C bounded paper loop blocked: {self:?}")
    }
}

impl std::error::Error for Stage5cPaperLoopError {}

pub struct Stage5cPaperLoopFailure {
    reason: Stage5cPaperLoopError,
    preserved_state: Option<Box<Stage5cPaperLoopState>>,
}

impl std::fmt::Debug for Stage5cPaperLoopFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cPaperLoopFailure")
            .field("reason", &self.reason)
            .field(
                "preserved_state_kind",
                &self.preserved_state.as_ref().map(|state| state.kind()),
            )
            .finish_non_exhaustive()
    }
}

impl Stage5cPaperLoopFailure {
    pub fn reason(&self) -> Stage5cPaperLoopError {
        self.reason
    }

    pub fn preserved_state(&self) -> Option<&Stage5cPaperLoopState> {
        self.preserved_state.as_deref()
    }

    pub fn into_preserved_state(self) -> Option<Stage5cPaperLoopState> {
        self.preserved_state.map(|state| *state)
    }
}

impl std::fmt::Display for Stage5cPaperLoopFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Stage 5C bounded paper loop failed: {:?}",
            self.reason
        )
    }
}

impl std::error::Error for Stage5cPaperLoopFailure {}

impl Stage5cPendingRecoveredPaperStrategy {
    pub fn receipt(&self) -> &Stage5cPendingRecoveryReceipt {
        &self.receipt
    }
    #[cfg(test)]
    fn strategy(&self) -> &HybridIntradayRuntimeStrategy {
        &self.strategy
    }

    pub(crate) fn into_parts(
        self,
    ) -> (HybridIntradayRuntimeStrategy, Stage5cPendingRecoveryReceipt) {
        (self.strategy, self.receipt)
    }
}

fn stage5cn_invalid_transition(
    state: Stage5cPaperLoopState,
    event: Stage5cPaperLoopEventKind,
) -> Stage5cPaperLoopFailure {
    Stage5cPaperLoopFailure {
        reason: Stage5cPaperLoopError::InvalidTransition {
            state: state.kind(),
            event,
        },
        preserved_state: Some(Box::new(state)),
    }
}

fn stage5cn_terminal(reason: Stage5cPaperLoopError) -> Stage5cPaperLoopFailure {
    Stage5cPaperLoopFailure {
        reason,
        preserved_state: None,
    }
}

fn stage5cn_preserved(
    reason: Stage5cPaperLoopError,
    state: Stage5cPaperLoopState,
) -> Stage5cPaperLoopFailure {
    Stage5cPaperLoopFailure {
        reason,
        preserved_state: Some(Box::new(state)),
    }
}

fn stage5cn_resolve_broker_event(
    resolved: Stage5cResolvedPaperIntentBatchStrategy,
    input: Stage5cPaperBrokerLifecycleInput,
) -> Result<Stage5cPaperLoopState, Stage5cPaperLoopFailure> {
    if let Err(reason) = stage5cn_require_terminal_broker_lifecycle_batch(&resolved, &input) {
        return Err(stage5cn_preserved(
            reason,
            Stage5cPaperLoopState::IntentLifecycleResolved(Box::new(resolved)),
        ));
    }
    resolve_stage5c_paper_broker_lifecycle(resolved, input)
        .map(|state| Stage5cPaperLoopState::BrokerLifecycleResolved(Box::new(state)))
        .map_err(|failure| {
            let reason = Stage5cPaperLoopError::BrokerLifecycle(failure.reason());
            match failure.into_blocked() {
                Some(blocked) => stage5cn_preserved(
                    reason,
                    Stage5cPaperLoopState::IntentLifecycleResolved(Box::new(
                        blocked.into_resolved(),
                    )),
                ),
                None => stage5cn_terminal(reason),
            }
        })
}

fn stage5cn_require_terminal_broker_lifecycle_batch(
    resolved: &Stage5cResolvedPaperIntentBatchStrategy,
    input: &Stage5cPaperBrokerLifecycleInput,
) -> Result<(), Stage5cPaperLoopError> {
    let mut sequences = HashSet::new();
    let mut event_identity_records: HashMap<String, Stage5cPaperBrokerEventRecord> = HashMap::new();
    for record in &input.event_records {
        if !sequences.insert(record.total_sequence) {
            return Err(Stage5cPaperLoopError::BrokerLifecycle(
                Stage5cPaperBrokerLifecycleError::DuplicateSequence,
            ));
        }
        if !resolved
            .resolved_batch
            .request_ids
            .contains(&record.request_id)
        {
            return Err(Stage5cPaperLoopError::BrokerLifecycle(
                Stage5cPaperBrokerLifecycleError::UnknownEventRequestId,
            ));
        }
        if record.payload.instrument() != resolved.resolved_batch.instrument() {
            return Err(Stage5cPaperLoopError::BrokerLifecycle(
                Stage5cPaperBrokerLifecycleError::InstrumentMismatch,
            ));
        }
        let identity = stage5cj_event_identity(record).map_err(|_| {
            Stage5cPaperLoopError::BrokerLifecycle(
                Stage5cPaperBrokerLifecycleError::CallbackValidationFailed,
            )
        })?;
        if let Some(previous) = event_identity_records.get_mut(&identity) {
            if record.payload != previous.payload {
                return Err(Stage5cPaperLoopError::BrokerLifecycle(
                    Stage5cPaperBrokerLifecycleError::ConflictingDuplicateEvent,
                ));
            }
            if record.total_sequence < previous.total_sequence {
                *previous = record.clone();
            }
            continue;
        }
        event_identity_records.insert(identity, record.clone());
    }

    let mut canonical_event_records: Vec<_> = event_identity_records.into_values().collect();
    canonical_event_records.sort_by_key(|record| record.total_sequence);
    let admission_strategy_id = resolved
        .recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission
        .strategy_id()
        .to_string();
    let ack_by_request: HashMap<StrategyRequestId, Stage5cPaperAckOutcome> = resolved
        .ack_outcomes
        .iter()
        .cloned()
        .map(|outcome| (outcome.request_id, outcome))
        .collect();
    let mut events_by_request: HashMap<StrategyRequestId, Vec<Stage5cPaperBrokerEventRecord>> =
        HashMap::new();
    for record in &canonical_event_records {
        events_by_request
            .entry(record.request_id)
            .or_default()
            .push(record.clone());
    }
    for intent_record in &resolved.resolved_batch.records {
        let ack = ack_by_request
            .get(&intent_record.request_id)
            .expect("ACK lifecycle enforces exact request coverage");
        let request_events = events_by_request
            .get(&intent_record.request_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if stage5cj_ack_is_terminal(ack.status) {
            if !request_events.is_empty() {
                return Err(Stage5cPaperLoopError::BrokerLifecycle(
                    Stage5cPaperBrokerLifecycleError::EventForTerminalAck,
                ));
            }
            continue;
        }
        if request_events.is_empty() {
            return Err(Stage5cPaperLoopError::BrokerLifecycle(
                Stage5cPaperBrokerLifecycleError::MissingExpectedBrokerEvent,
            ));
        }
        let mut terminal_seen = false;
        let mut simulated_position_qty = stage5cj_position_qty(Strategy::state(&resolved.strategy));
        for record in request_events {
            if record.payload.source_ts_utc() < ack.processed_ts_utc {
                return Err(Stage5cPaperLoopError::BrokerLifecycle(
                    Stage5cPaperBrokerLifecycleError::EventTimestampBeforeAck,
                ));
            }
            if !stage5cj_allowed_event_kinds(intent_record).contains(&record.payload.kind()) {
                return Err(Stage5cPaperLoopError::BrokerLifecycle(
                    Stage5cPaperBrokerLifecycleError::UnexpectedBrokerEventKind,
                ));
            }
            stage5cj_validate_event_mapping(
                record,
                ack,
                intent_record,
                &admission_strategy_id,
                simulated_position_qty,
            )
            .map_err(|failure| match failure {
                Stage5cPaperBrokerLifecycleFailure::Blocked(blocked) => {
                    Stage5cPaperLoopError::BrokerLifecycle(blocked.reason())
                }
                Stage5cPaperBrokerLifecycleFailure::Terminal(reason) => {
                    Stage5cPaperLoopError::BrokerLifecycle(reason)
                }
            })?;
            if stage5cj_event_is_terminal_for_intent(record, intent_record, request_events) {
                terminal_seen = true;
            }
            if let Stage5cPaperBrokerEventPayload::Position(position) = &record.payload {
                simulated_position_qty = position.qty;
            }
        }
        if !terminal_seen {
            return Err(Stage5cPaperLoopError::BrokerLifecycleIncompleteBatch);
        }
    }
    Ok(())
}

fn stage5cn_resolve_timer(
    resolved: Stage5cBrokerLifecycleResolvedPaperStrategy,
    input: Stage5cPaperTimerInput,
) -> Result<Stage5cPaperLoopState, Stage5cPaperLoopFailure> {
    resolve_stage5c_paper_timer(resolved, input)
        .map(|state| Stage5cPaperLoopState::TimerResolved(Box::new(state)))
        .map_err(|failure| {
            let reason = Stage5cPaperLoopError::Timer(failure.reason());
            match failure.into_blocked() {
                Some(blocked) => stage5cn_preserved(
                    reason,
                    Stage5cPaperLoopState::BrokerLifecycleResolved(Box::new(
                        blocked.into_resolved(),
                    )),
                ),
                None => stage5cn_terminal(reason),
            }
        })
}

pub fn settle_stage5c_broker_lifecycle_result(
    resolved: Stage5cBrokerLifecycleResolvedPaperStrategy,
) -> Stage5cBrokerLifecycleSettlement {
    if !resolved.remaining_lifecycle_expectations.is_empty() {
        return Stage5cBrokerLifecycleSettlement::unresolved_broker_lifecycle(resolved);
    }
    let Stage5cBrokerLifecycleResolvedPaperStrategy {
        strategy,
        recovery_receipt,
        resolved_batch,
        resolved_batch_summary,
        ack_outcomes,
        broker_event_count,
        remaining_lifecycle_expectations,
        lifecycle_watermark_ts_utc,
        generated_intent_batch,
        settled_batch_history,
    } = resolved;
    match generated_intent_batch {
        Some(batch) => {
            Stage5cBrokerLifecycleSettlement::generated_intent_batch(Stage5cSettledPaperStrategy {
                strategy,
                recovery_receipt,
                batch,
                settled_batch_history,
            })
        }
        None => Stage5cBrokerLifecycleSettlement::ready_for_timer(
            Stage5cBrokerLifecycleResolvedPaperStrategy {
                strategy,
                recovery_receipt,
                resolved_batch,
                resolved_batch_summary,
                ack_outcomes,
                broker_event_count,
                remaining_lifecycle_expectations,
                lifecycle_watermark_ts_utc,
                generated_intent_batch: None,
                settled_batch_history,
            },
        ),
    }
}

pub fn advance_stage5c_paper_loop_once(
    state: Stage5cPaperLoopState,
    event: Stage5cPaperLoopEvent,
) -> Result<Stage5cPaperLoopState, Stage5cPaperLoopFailure> {
    let event_kind = event.kind();
    match (state, event) {
        (
            Stage5cPaperLoopState::PendingRecovered(recovered),
            Stage5cPaperLoopEvent::FinalM10Bar(bar),
        ) => apply_stage5c_semantic_bar(*recovered, *bar)
            .map(|state| Stage5cPaperLoopState::SemanticResult(Box::new(state)))
            .map_err(|reason| stage5cn_terminal(Stage5cPaperLoopError::Semantic(reason))),
        (
            Stage5cPaperLoopState::SemanticResult(result),
            Stage5cPaperLoopEvent::SettleSemanticResult,
        ) => settle_stage5c_semantic_result(*result)
            .map(|state| Stage5cPaperLoopState::Settled(Box::new(state)))
            .map_err(|reason| stage5cn_terminal(Stage5cPaperLoopError::IntentSettlement(reason))),
        (Stage5cPaperLoopState::Settled(settled), Stage5cPaperLoopEvent::FinalM10Bar(bar)) => {
            advance_stage5c_controlled_next_bar(*settled, *bar)
                .map(|state| Stage5cPaperLoopState::Settled(Box::new(state)))
                .map_err(|failure| {
                    let reason = Stage5cPaperLoopError::NextBar(failure.reason());
                    match failure.into_blocked() {
                        Some(blocked) => stage5cn_preserved(
                            reason,
                            Stage5cPaperLoopState::Settled(Box::new(blocked.into_settled())),
                        ),
                        None => stage5cn_terminal(reason),
                    }
                })
        }
        (Stage5cPaperLoopState::Settled(settled), Stage5cPaperLoopEvent::Ack(input)) => {
            resolve_stage5c_paper_intent_lifecycle(*settled, *input)
                .map(|state| Stage5cPaperLoopState::IntentLifecycleResolved(Box::new(state)))
                .map_err(|failure| {
                    let reason = Stage5cPaperLoopError::IntentLifecycle(failure.reason());
                    match failure.into_blocked() {
                        Some(blocked) => stage5cn_preserved(
                            reason,
                            Stage5cPaperLoopState::Settled(Box::new(blocked.into_settled())),
                        ),
                        None => stage5cn_terminal(reason),
                    }
                })
        }
        (
            Stage5cPaperLoopState::IntentLifecycleResolved(resolved),
            Stage5cPaperLoopEvent::BrokerLifecycleBatch(input),
        ) => stage5cn_resolve_broker_event(*resolved, *input),
        (
            Stage5cPaperLoopState::BrokerLifecycleResolved(resolved),
            Stage5cPaperLoopEvent::SettleBrokerLifecycleResult,
        ) => Ok(Stage5cPaperLoopState::BrokerLifecycleSettlement(Box::new(
            settle_stage5c_broker_lifecycle_result(*resolved),
        ))),
        (
            Stage5cPaperLoopState::BrokerLifecycleSettlement(settlement),
            Stage5cPaperLoopEvent::Ack(input),
        ) => match settlement.inner {
            Stage5cBrokerLifecycleSettlementKind::GeneratedIntentBatch(settled) => {
                resolve_stage5c_paper_intent_lifecycle(settled, *input)
                    .map(|state| Stage5cPaperLoopState::IntentLifecycleResolved(Box::new(state)))
                    .map_err(|failure| {
                        let reason = Stage5cPaperLoopError::IntentLifecycle(failure.reason());
                        match failure.into_blocked() {
                            Some(blocked) => stage5cn_preserved(
                                reason,
                                Stage5cPaperLoopState::Settled(Box::new(blocked.into_settled())),
                            ),
                            None => stage5cn_terminal(reason),
                        }
                    })
            }
            Stage5cBrokerLifecycleSettlementKind::ReadyForTimer(resolved) => {
                Err(stage5cn_invalid_transition(
                    Stage5cPaperLoopState::BrokerLifecycleSettlement(Box::new(
                        Stage5cBrokerLifecycleSettlement::ready_for_timer(resolved),
                    )),
                    Stage5cPaperLoopEventKind::Ack,
                ))
            }
            Stage5cBrokerLifecycleSettlementKind::UnresolvedBrokerLifecycle(resolved) => {
                Err(stage5cn_preserved(
                    Stage5cPaperLoopError::BrokerLifecycleUnresolved,
                    Stage5cPaperLoopState::BrokerLifecycleSettlement(Box::new(
                        Stage5cBrokerLifecycleSettlement::unresolved_broker_lifecycle(resolved),
                    )),
                ))
            }
        },
        (
            Stage5cPaperLoopState::BrokerLifecycleSettlement(settlement),
            Stage5cPaperLoopEvent::Timer(input),
        ) => match settlement.inner {
            Stage5cBrokerLifecycleSettlementKind::ReadyForTimer(resolved) => {
                stage5cn_resolve_timer(resolved, input)
            }
            Stage5cBrokerLifecycleSettlementKind::GeneratedIntentBatch(settled) => {
                Err(stage5cn_preserved(
                    Stage5cPaperLoopError::BrokerLifecycleRequiresGeneratedAck,
                    Stage5cPaperLoopState::BrokerLifecycleSettlement(Box::new(
                        Stage5cBrokerLifecycleSettlement::generated_intent_batch(settled),
                    )),
                ))
            }
            Stage5cBrokerLifecycleSettlementKind::UnresolvedBrokerLifecycle(resolved) => {
                Err(stage5cn_preserved(
                    Stage5cPaperLoopError::BrokerLifecycleUnresolved,
                    Stage5cPaperLoopState::BrokerLifecycleSettlement(Box::new(
                        Stage5cBrokerLifecycleSettlement::unresolved_broker_lifecycle(resolved),
                    )),
                ))
            }
        },
        (
            Stage5cPaperLoopState::BrokerLifecycleResolved(resolved),
            Stage5cPaperLoopEvent::Timer(input),
        ) => stage5cn_resolve_timer(*resolved, input),
        (Stage5cPaperLoopState::TimerResolved(timer), Stage5cPaperLoopEvent::SettleTimerResult) => {
            Ok(Stage5cPaperLoopState::TimerSettlement(Box::new(
                settle_stage5c_timer_result(*timer),
            )))
        }
        (
            Stage5cPaperLoopState::TimerSettlement(settlement),
            Stage5cPaperLoopEvent::FinalM10Bar(bar),
        ) => advance_stage5c_timer_settlement_next_bar(*settlement, *bar)
            .map(|state| Stage5cPaperLoopState::Settled(Box::new(state)))
            .map_err(|failure| {
                let reason = Stage5cPaperLoopError::TimerContinuation(failure.reason());
                match failure.into_blocked() {
                    Some(blocked) => stage5cn_preserved(
                        reason,
                        Stage5cPaperLoopState::TimerSettlement(Box::new(blocked.into_settlement())),
                    ),
                    None => stage5cn_terminal(reason),
                }
            }),
        (
            Stage5cPaperLoopState::TimerSettlement(settlement),
            Stage5cPaperLoopEvent::Timer(input),
        ) => advance_stage5c_timer_settlement_timer(*settlement, input)
            .map(|state| Stage5cPaperLoopState::TimerResolved(Box::new(state)))
            .map_err(|failure| {
                let reason = Stage5cPaperLoopError::TimerContinuation(failure.reason());
                match failure.into_blocked() {
                    Some(blocked) => stage5cn_preserved(
                        reason,
                        Stage5cPaperLoopState::TimerSettlement(Box::new(blocked.into_settlement())),
                    ),
                    None => stage5cn_terminal(reason),
                }
            }),
        (Stage5cPaperLoopState::TimerSettlement(settlement), Stage5cPaperLoopEvent::Ack(input)) => {
            match (*settlement).into_generated_intent_batch() {
                Ok(settled) => resolve_stage5c_paper_intent_lifecycle(settled, *input)
                    .map(|state| Stage5cPaperLoopState::IntentLifecycleResolved(Box::new(state)))
                    .map_err(|failure| {
                        let reason = Stage5cPaperLoopError::IntentLifecycle(failure.reason());
                        match failure.into_blocked() {
                            Some(blocked) => stage5cn_preserved(
                                reason,
                                Stage5cPaperLoopState::Settled(Box::new(blocked.into_settled())),
                            ),
                            None => stage5cn_terminal(reason),
                        }
                    }),
                Err(settlement) => Err(stage5cn_invalid_transition(
                    Stage5cPaperLoopState::TimerSettlement(settlement),
                    Stage5cPaperLoopEventKind::Ack,
                )),
            }
        }
        (state, _) => Err(stage5cn_invalid_transition(state, event_kind)),
    }
}

impl Stage5cPaperHostAdmission {
    pub fn schema_version(&self) -> u16 {
        self.schema_version
    }

    pub fn checked_ts(&self) -> DateTime<Utc> {
        self.checked_ts
    }

    pub fn issued_ts(&self) -> DateTime<Utc> {
        self.issued_ts
    }

    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    pub fn account_id(&self) -> &BrokerAccountId {
        &self.account_id
    }

    pub fn strategy_id(&self) -> &str {
        &self.strategy_id
    }

    pub fn target_instrument(&self) -> &InstrumentId {
        &self.target_instrument
    }

    pub fn tick_size(&self) -> f64 {
        self.tick_size
    }

    pub fn bootstrap_snapshot(&self) -> &RuntimeHostBootstrapSnapshot {
        &self.bootstrap_snapshot
    }

    pub fn is_paper_only(&self) -> bool {
        self.paper_only
    }

    pub fn runtime_host_attached(&self) -> bool {
        self.runtime_host_attached
    }

    pub fn intent_sink_attached(&self) -> bool {
        self.intent_sink_attached
    }
}

pub fn admit_stage5c_paper_host(
    input: Stage5cPaperHostAdmissionInput<'_>,
) -> Result<Stage5cPaperHostAdmission, Stage5cPaperHostAdmissionError> {
    admit_stage5c_paper_host_at(input, Utc::now())
}

pub(crate) fn admit_stage5c_paper_host_at(
    input: Stage5cPaperHostAdmissionInput<'_>,
    admission_now: DateTime<Utc>,
) -> Result<Stage5cPaperHostAdmission, Stage5cPaperHostAdmissionError> {
    if input.strategy_id.trim().is_empty() {
        return Err(Stage5cPaperHostAdmissionError::StrategyIdEmpty);
    }
    let report = input.stage4_evidence.report();
    if report.checked_ts > admission_now {
        return Err(Stage5cPaperHostAdmissionError::EvidenceCheckedInFuture);
    }
    if admission_now > input.stage4_evidence.required_source_expires_at() {
        return Err(Stage5cPaperHostAdmissionError::EvidenceExpired);
    }
    if report.schema_version != STAGE4_BOOTSTRAP_EVIDENCE_REPORT_SCHEMA_VERSION {
        return Err(Stage5cPaperHostAdmissionError::Stage4ReportSchemaMismatch);
    }
    if report.status != Stage4BootstrapEvidenceReportStatus::Accepted {
        return Err(Stage5cPaperHostAdmissionError::Stage4ReportNotAccepted);
    }
    let expected_events = [
        Stage4RuntimeBootstrapIntegrationEvent::NotifyBootstrapSnapshot,
        Stage4RuntimeBootstrapIntegrationEvent::NotifyRuntimeStateRestored,
        Stage4RuntimeBootstrapIntegrationEvent::WarmupHistory,
        Stage4RuntimeBootstrapIntegrationEvent::RecoverPendingStreams,
    ];
    if report.stage4c_status != Stage4BrokerTruthBootstrapStatus::BootstrapReady
        || report.broker_truth_source_status != Stage4BrokerTruthSourceStatus::Present
        || !report.stage4c_blocker_kinds.is_empty()
        || report.stage4e_status != Stage4RuntimeBootstrapApplicationStatus::Applied
        || !report.stage4e_blocker_kinds.is_empty()
        || report.stage4f_status != Stage4DirtyStartPolicyStatus::Accepted
        || !report.stage4f_blocker_kinds.is_empty()
        || report.stage4g_status != Stage4RuntimeLifecycleOrderingStatus::Accepted
        || !report.stage4g_blocker_kinds.is_empty()
        || !report.stage4g_lifecycle_issues.is_empty()
        || report.stage4h_status != Stage4RuntimeBootstrapIntegrationStatus::Accepted
        || !report.stage4h_blocker_kinds.is_empty()
        || !report.reason_chain.is_empty()
        || report.blocker_count != 0
        || report.manual_intervention_required
        || !report.runtime_events_emitted
        || report.mock_runtime_events != expected_events
    {
        return Err(Stage5cPaperHostAdmissionError::Stage4EvidenceChainInconsistent);
    }
    let expected_source_sections = HashSet::from([
        Stage4BrokerTruthFreshnessSection::Positions,
        Stage4BrokerTruthFreshnessSection::Orders,
        Stage4BrokerTruthFreshnessSection::Trades,
        Stage4BrokerTruthFreshnessSection::Cash,
        Stage4BrokerTruthFreshnessSection::Instruments,
        Stage4BrokerTruthFreshnessSection::Schedule,
    ]);
    let actual_source_sections = report
        .source_sections
        .iter()
        .map(|section| section.section)
        .collect::<HashSet<_>>();
    if report.source_sections.len() != expected_source_sections.len()
        || actual_source_sections != expected_source_sections
        || report.source_sections.iter().any(|section| {
            section.blocks_bootstrap
                || (section.required_for_bootstrap
                    && (section.source_status != Stage4BrokerTruthSourceStatus::Present
                        || section.freshness_status != Stage4BrokerTruthFreshnessStatus::Fresh))
        })
    {
        return Err(Stage5cPaperHostAdmissionError::Stage4EvidenceChainInconsistent);
    }
    if !report.no_live_authorization
        || report.safety_boundary.runtime_live_enabled
        || report.safety_boundary.real_finam_command_consumer_enabled
        || report.safety_boundary.strategy_driven_real_orders_enabled
        || report.safety_boundary.real_post_delete_enabled
        || report.safety_boundary.stop_sltp_bracket_enabled
        || report.safety_boundary.raw_payload_exported
        || !report.redaction.report_redacted
        || report.redaction.raw_payloads_exported
        || report.redaction.secrets_exported
        || report.redaction.account_sensitive_dumps_exported
        || report.redaction.broker_account_id_exported
        || report.redaction.raw_order_comments_exported
    {
        return Err(Stage5cPaperHostAdmissionError::Stage4SafetyBoundaryOpen);
    }

    let application = input.stage4_evidence.application();
    if application.schema_version != STAGE4_RUNTIME_BOOTSTRAP_APPLICATION_SCHEMA_VERSION {
        return Err(Stage5cPaperHostAdmissionError::Stage4ApplicationSchemaMismatch);
    }
    if application.status != Stage4RuntimeBootstrapApplicationStatus::Applied {
        return Err(Stage5cPaperHostAdmissionError::Stage4ApplicationNotApplied);
    }
    if application.source_bootstrap_status != Stage4BrokerTruthBootstrapStatus::BootstrapReady
        || !application.blockers.is_empty()
        || application.blocker_count != 0
        || !application.broker_truth_loaded_before_runtime_state
        || application.restored_runtime_state_accepted_after_broker_truth
            != application.restored_runtime_state_present
        || application.restored_runtime_overrode_broker_truth
        || !application.no_live_authorization
    {
        return Err(Stage5cPaperHostAdmissionError::Stage4ApplicationInconsistent);
    }
    let snapshot = input.stage4_evidence.applied_snapshot();
    if application.applied_snapshot.as_ref() != Some(snapshot) {
        return Err(Stage5cPaperHostAdmissionError::Stage4ApplicationSnapshotMissing);
    }
    if application.checked_ts != report.checked_ts
        || application.target_position_qty != snapshot.target_position_qty
        || application.target_is_flat != snapshot.target_is_flat
        || application.target_active_order_count != snapshot.target_active_orders.len()
        || application.account_active_order_count != snapshot.account_active_orders_count
        || report.target_is_flat != snapshot.target_is_flat
        || report.target_active_order_count != snapshot.target_active_orders.len()
        || report.account_active_order_count != snapshot.account_active_orders_count
    {
        return Err(Stage5cPaperHostAdmissionError::Stage4ReportApplicationMismatch);
    }

    if report.target_instrument != snapshot.instrument
        || report.target_instrument != *input.configured_target_instrument
    {
        return Err(Stage5cPaperHostAdmissionError::TargetInstrumentMismatch);
    }
    if snapshot.account_id != *input.configured_account_id {
        return Err(Stage5cPaperHostAdmissionError::AccountScopeMismatch);
    }
    if input.instrument_spec.instrument_id() != report.target_instrument {
        return Err(Stage5cPaperHostAdmissionError::InstrumentSpecMismatch);
    }
    let spec_tick_size = input
        .instrument_spec
        .instrument
        .price_step
        .to_f64()
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or(Stage5cPaperHostAdmissionError::InvalidInstrumentPriceStep)?;
    if !input.configured_tick_size.is_finite()
        || input.configured_tick_size <= 0.0
        || (input.configured_tick_size - spec_tick_size).abs() > f64::EPSILON
    {
        return Err(Stage5cPaperHostAdmissionError::TickSizeMismatch);
    }
    if input.allow_live_orders {
        return Err(Stage5cPaperHostAdmissionError::LiveOrdersRequested);
    }

    Ok(Stage5cPaperHostAdmission {
        schema_version: STAGE5C_PAPER_HOST_ADMISSION_SCHEMA_VERSION,
        checked_ts: report.checked_ts,
        issued_ts: admission_now,
        expires_at: input.stage4_evidence.required_source_expires_at(),
        strategy_id: input.strategy_id,
        account_id: snapshot.account_id.clone(),
        target_instrument: report.target_instrument.clone(),
        tick_size: spec_tick_size,
        bootstrap_snapshot: snapshot.clone(),
        paper_only: true,
        runtime_host_attached: false,
        intent_sink_attached: false,
    })
}

pub fn prepare_stage5c_without_runtime_state(
    strategy: HybridIntradayRuntimeStrategy,
    admission: Stage5cPaperHostAdmission,
) -> Stage5cRuntimeStateLoadedPaperStrategy {
    Stage5cRuntimeStateLoadedPaperStrategy {
        strategy,
        admission,
        restored: RuntimeStateRestored {
            known_order_ids: Vec::new(),
            pending_requests: Vec::new(),
        },
        load_origin: Stage5cRuntimeStateLoadOrigin::CleanStart,
    }
}

/// Consumes the state-loaded type-state, preventing duplicate notification.
///
/// ```compile_fail
/// # use strategy_runtime_core::{notify_stage5c_bootstrap, Stage5cRuntimeStateLoadedPaperStrategy};
/// # fn duplicate(loaded: Stage5cRuntimeStateLoadedPaperStrategy) {
/// let _ = notify_stage5c_bootstrap(loaded);
/// let _ = notify_stage5c_bootstrap(loaded);
/// # }
/// ```
pub fn notify_stage5c_bootstrap(
    loaded: Stage5cRuntimeStateLoadedPaperStrategy,
) -> Result<Stage5cBootstrappedPaperStrategy, Stage5cBootstrapNotificationError> {
    notify_stage5c_bootstrap_at(loaded, Utc::now())
}

pub(crate) fn notify_stage5c_bootstrap_at(
    loaded: Stage5cRuntimeStateLoadedPaperStrategy,
    notification_now: DateTime<Utc>,
) -> Result<Stage5cBootstrappedPaperStrategy, Stage5cBootstrapNotificationError> {
    let Stage5cRuntimeStateLoadedPaperStrategy {
        mut strategy,
        admission,
        restored,
        load_origin: _,
    } = loaded;
    validate_stage5cb_notification(&strategy, &admission, notification_now)?;
    let snapshot = admission.bootstrap_snapshot();
    let position_qty = snapshot
        .target_position_qty
        .to_f64()
        .filter(|value| value.is_finite())
        .ok_or(Stage5cBootstrapNotificationError::PositionQuantityNotRepresentable)?;
    let average_price = snapshot
        .target_open_positions
        .first()
        .and_then(|position| position.avg_price)
        .map(|price| {
            price
                .to_f64()
                .filter(|value| value.is_finite())
                .ok_or(Stage5cBootstrapNotificationError::PositionAveragePriceNotRepresentable)
        })
        .transpose()?
        .unwrap_or_default();
    let mut positions_strategy = HashMap::new();
    if !snapshot.target_open_positions.is_empty() || position_qty.abs() > f64::EPSILON {
        positions_strategy.insert(
            snapshot.instrument.symbol.clone(),
            PositionEvent {
                symbol: snapshot.instrument.symbol.clone(),
                qty: position_qty,
                existing: true,
                avg_price: average_price,
                ts_utc: snapshot.received_ts.timestamp(),
            },
        );
    }
    let source_snapshot = BootstrapSnapshot {
        positions_strategy,
        working_orders_strategy: HashMap::new(),
        working_stop_orders_strategy: HashMap::new(),
        snapshot_ts_utc: Some(snapshot.received_ts.timestamp()),
    };
    let context = StrategyCtx {
        strategy_id: admission.strategy_id().to_string(),
        portfolio: admission.account_id().as_str().to_string(),
        exchange: format!("{:?}", admission.target_instrument().exchange),
        symbol: admission.target_instrument().symbol.clone(),
        tick_size: admission.tick_size(),
        trade_mode: TradeMode::Paper,
        paper_execution_mode: PaperExecutionMode::LiveOnly,
        allow_live_orders: false,
        gateway_phase: GatewayPhase::SyncingHistory,
        position_qty: Some(position_qty),
        event_ts_utc: snapshot.received_ts.timestamp(),
        now_ts_utc: notification_now.timestamp(),
        last_bar_ts: None,
    };
    let intents = Strategy::on_bootstrap_snapshot(&mut strategy, &context, &source_snapshot);
    debug_assert!(
        intents.is_empty(),
        "accepted source bootstrap callback must not emit intents"
    );

    Ok(Stage5cBootstrappedPaperStrategy {
        strategy,
        receipt: Stage5cBootstrapNotificationReceipt {
            admission,
            notified_ts: notification_now,
        },
        restored,
    })
}

fn validate_stage5cb_notification(
    strategy: &HybridIntradayRuntimeStrategy,
    admission: &Stage5cPaperHostAdmission,
    notification_now: DateTime<Utc>,
) -> Result<(), Stage5cBootstrapNotificationError> {
    if notification_now > admission.expires_at() {
        return Err(Stage5cBootstrapNotificationError::AdmissionExpired);
    }
    let (symbol_matches, tick_size_matches) =
        strategy.stage5c_binding_matches(admission.target_instrument(), admission.tick_size());
    if !symbol_matches {
        return Err(Stage5cBootstrapNotificationError::StrategyTargetMismatch);
    }
    if !tick_size_matches {
        return Err(Stage5cBootstrapNotificationError::StrategyTickSizeMismatch);
    }
    let snapshot = admission.bootstrap_snapshot();
    if !snapshot.target_active_orders.is_empty() {
        return Err(Stage5cBootstrapNotificationError::ActiveOrdersRequireOwnershipMapping);
    }
    if snapshot.account_id != *admission.account_id() {
        return Err(Stage5cBootstrapNotificationError::SnapshotAccountMismatch);
    }
    if snapshot.instrument != *admission.target_instrument() {
        return Err(Stage5cBootstrapNotificationError::SnapshotInstrumentMismatch);
    }
    if snapshot.target_open_positions.iter().any(|position| {
        position.account_id != snapshot.account_id || position.instrument != snapshot.instrument
    }) {
        return Err(Stage5cBootstrapNotificationError::SnapshotInstrumentMismatch);
    }
    Ok(())
}

/// Validates and loads persisted semantic state before broker truth bootstrap.
pub fn restore_stage5c_runtime_state(
    strategy: HybridIntradayRuntimeStrategy,
    admission: Stage5cPaperHostAdmission,
    input: Stage5cRuntimeStateRestoreInput,
) -> Result<Stage5cRuntimeStateLoadedPaperStrategy, Stage5cRuntimeStateRestoreError> {
    restore_stage5c_runtime_state_at(strategy, admission, input, Utc::now())
}

fn restore_stage5c_runtime_state_at(
    mut strategy: HybridIntradayRuntimeStrategy,
    admission: Stage5cPaperHostAdmission,
    input: Stage5cRuntimeStateRestoreInput,
    restored_ts: DateTime<Utc>,
) -> Result<Stage5cRuntimeStateLoadedPaperStrategy, Stage5cRuntimeStateRestoreError> {
    if input.schema_version != STAGE5C_RUNTIME_STATE_RESTORE_SCHEMA_VERSION {
        return Err(Stage5cRuntimeStateRestoreError::SchemaMismatch);
    }
    if input.state_schema_version != 1 {
        return Err(Stage5cRuntimeStateRestoreError::StateSchemaMismatch);
    }
    if input.strategy_kind != "hybrid_intraday_runtime" {
        return Err(Stage5cRuntimeStateRestoreError::StrategyKindMismatch);
    }
    if restored_ts > admission.expires_at() {
        return Err(Stage5cRuntimeStateRestoreError::AdmissionExpired);
    }
    if input.persisted_ts > restored_ts {
        return Err(Stage5cRuntimeStateRestoreError::PersistedStateFromFuture);
    }
    if input.strategy_id != admission.strategy_id() {
        return Err(Stage5cRuntimeStateRestoreError::StrategyIdMismatch);
    }
    if input.account_id != *admission.account_id() {
        return Err(Stage5cRuntimeStateRestoreError::AccountMismatch);
    }
    if input.instrument != *admission.target_instrument() {
        return Err(Stage5cRuntimeStateRestoreError::InstrumentMismatch);
    }
    if !same_tick_size(input.tick_size, admission.tick_size()) {
        return Err(Stage5cRuntimeStateRestoreError::TickSizeMismatch);
    }
    if input.config_fingerprint != strategy.stage5c_config_fingerprint() {
        return Err(Stage5cRuntimeStateRestoreError::ConfigFingerprintMismatch);
    }
    let profile = strategy.stage5c_profile_binding();
    if (
        input.profile,
        input.mr_variant,
        input.mr_gate_policy,
        input.risk_gate_mode,
    ) != profile
    {
        return Err(Stage5cRuntimeStateRestoreError::ProfileBindingMismatch);
    }

    let mut raw_state: serde_json::Value = serde_json::from_str(&input.state_json)
        .map_err(|_| Stage5cRuntimeStateRestoreError::InvalidStateJson)?;
    normalize_legacy_order_ids(&mut raw_state, input.legacy_numeric_order_id_policy)?;
    let restored_state: StrategyState = serde_json::from_value(raw_state)
        .map_err(|_| Stage5cRuntimeStateRestoreError::InvalidStateJson)?;
    let (restored_position_qty, restored_side) = match &restored_state {
        StrategyState::HybridIntradayRuntime {
            last_position_qty,
            current_side,
            ..
        } => (*last_position_qty, *current_side),
        StrategyState::Idle => {
            return Err(Stage5cRuntimeStateRestoreError::WrongStrategyStateKind);
        }
    };
    let broker_position_qty = admission
        .bootstrap_snapshot()
        .target_position_qty
        .to_f64()
        .ok_or(Stage5cRuntimeStateRestoreError::BrokerTruthPositionMismatch)?;
    if (restored_position_qty - broker_position_qty).abs() > f64::EPSILON {
        return Err(Stage5cRuntimeStateRestoreError::BrokerTruthPositionMismatch);
    }
    let expected_side = if broker_position_qty > f64::EPSILON {
        Some(crate::hybrid_intraday::Side::Long)
    } else if broker_position_qty < -f64::EPSILON {
        Some(crate::hybrid_intraday::Side::Short)
    } else {
        None
    };
    if expected_side.is_some() && restored_side.is_some() && restored_side != expected_side {
        return Err(Stage5cRuntimeStateRestoreError::BrokerTruthSideMismatch);
    }

    let semantic_payload_fingerprint = stage5c_semantic_payload_fingerprint(&restored_state)
        .map_err(|_| Stage5cRuntimeStateRestoreError::InvalidStateJson)?;
    let recovery_index_fingerprint =
        stage5c_recovery_index_fingerprint(&input.known_order_ids, &input.pending_requests)
            .map_err(|_| Stage5cRuntimeStateRestoreError::InvalidStateJson)?;
    let persisted_ts = input.persisted_ts;
    Strategy::set_state(&mut strategy, restored_state);
    Ok(Stage5cRuntimeStateLoadedPaperStrategy {
        strategy,
        admission,
        restored: RuntimeStateRestored {
            known_order_ids: input.known_order_ids,
            pending_requests: input.pending_requests,
        },
        load_origin: Stage5cRuntimeStateLoadOrigin::Persisted {
            semantic_payload_fingerprint,
            persisted_ts,
            recovery_index_fingerprint,
        },
    })
}

pub(crate) fn stage5c_semantic_payload_fingerprint(
    state: &StrategyState,
) -> Result<String, serde_json::Error> {
    let value = serde_json::to_value(state)?;
    let value = stage5c_persisted_owned_semantic_projection(value);
    let payload = serde_json::to_vec(&value)?;
    Ok(format!(
        "stage5c_semantic_sha256:{:x}",
        Sha256::digest(payload)
    ))
}

pub(crate) fn stage5c_semantic_value_fingerprint(
    value: &serde_json::Value,
) -> Result<String, serde_json::Error> {
    let value = stage5c_persisted_owned_semantic_projection(value.clone());
    let payload = serde_json::to_vec(&value)?;
    Ok(format!(
        "stage5c_semantic_sha256:{:x}",
        Sha256::digest(payload)
    ))
}

fn stage5c_persisted_owned_semantic_projection(mut value: serde_json::Value) -> serde_json::Value {
    if let Some(fields) = value
        .get_mut("HybridIntradayRuntime")
        .and_then(|state| state.as_object_mut())
    {
        for recomputable in [
            "entry_ready",
            "last_bar_close",
            "prev_day_close",
            "last_day_local",
            "current_day_high",
            "current_day_low",
            "current_day_close",
            "prev_day_range",
            "prev_day_return",
            "day_before_close",
            "today_start_local",
            "risk_gate_mr_enabled_current_session",
            "risk_gate_rolling_sum_lb120",
            "risk_gate_last_finalized_session_date",
            "risk_gate_ledger_rows_count",
        ] {
            fields.remove(recomputable);
        }
    }
    value
}

pub(crate) fn stage5c_recovery_index_fingerprint(
    known_order_ids: &[BrokerOrderId],
    pending_requests: &[StrategyRequestId],
) -> Result<String, serde_json::Error> {
    let mut known_order_ids: Vec<_> = known_order_ids
        .iter()
        .map(|id| id.as_str().to_string())
        .collect();
    known_order_ids.sort();
    let mut pending_requests: Vec<_> = pending_requests
        .iter()
        .map(|request| request.0.to_string())
        .collect();
    pending_requests.sort();
    let payload = serde_json::json!({
        "known_order_ids": known_order_ids,
        "pending_requests": pending_requests,
    });
    let payload = serde_json::to_vec(&payload)?;
    Ok(format!(
        "stage5c_recovery_sha256:{:x}",
        Sha256::digest(payload)
    ))
}

pub fn notify_stage5c_runtime_state_restored(
    bootstrapped: Stage5cBootstrappedPaperStrategy,
) -> Result<Stage5cRuntimeStateRestoredPaperStrategy, Stage5cRuntimeStateRestoreError> {
    notify_stage5c_runtime_state_restored_at(bootstrapped, Utc::now())
}

fn notify_stage5c_runtime_state_restored_at(
    bootstrapped: Stage5cBootstrappedPaperStrategy,
    restored_ts: DateTime<Utc>,
) -> Result<Stage5cRuntimeStateRestoredPaperStrategy, Stage5cRuntimeStateRestoreError> {
    let (mut strategy, bootstrap_receipt, restored) = bootstrapped.into_parts();
    let admission = &bootstrap_receipt.admission;
    let broker_position_qty = admission
        .bootstrap_snapshot()
        .target_position_qty
        .to_f64()
        .ok_or(Stage5cRuntimeStateRestoreError::BrokerTruthPositionMismatch)?;
    let context = StrategyCtx {
        strategy_id: admission.strategy_id().to_string(),
        portfolio: admission.account_id().as_str().to_string(),
        exchange: format!("{:?}", admission.target_instrument().exchange),
        symbol: admission.target_instrument().symbol.clone(),
        tick_size: admission.tick_size(),
        trade_mode: TradeMode::Paper,
        paper_execution_mode: PaperExecutionMode::LiveOnly,
        allow_live_orders: false,
        gateway_phase: GatewayPhase::SyncingHistory,
        position_qty: Some(broker_position_qty),
        event_ts_utc: restored_ts.timestamp(),
        now_ts_utc: restored_ts.timestamp(),
        last_bar_ts: None,
    };
    let known_order_ids = restored.known_order_ids.clone();
    let pending_requests = restored.pending_requests.clone();
    let intents = Strategy::on_runtime_state_restored(&mut strategy, &context, &restored);
    debug_assert!(
        intents.is_empty(),
        "accepted source runtime-state restore must not emit intents"
    );
    validate_post_bootstrap_broker_truth(&strategy, admission)?;

    Ok(Stage5cRuntimeStateRestoredPaperStrategy {
        strategy,
        receipt: Stage5cRuntimeStateRestoreReceipt {
            bootstrap_receipt,
            restored_ts,
            known_order_ids,
            pending_requests,
        },
    })
}

fn same_tick_size(left: f64, right: f64) -> bool {
    left.is_finite() && right.is_finite() && (left - right).abs() <= f64::EPSILON
}

fn normalize_legacy_order_ids(
    value: &mut serde_json::Value,
    policy: Stage5cLegacyNumericOrderIdPolicy,
) -> Result<(), Stage5cRuntimeStateRestoreError> {
    match value {
        serde_json::Value::Object(fields) => {
            for (name, field) in fields {
                if matches!(name.as_str(), "tp_order_id" | "sl_exchange_order_id")
                    && field.is_number()
                {
                    if policy == Stage5cLegacyNumericOrderIdPolicy::Reject {
                        return Err(Stage5cRuntimeStateRestoreError::LegacyNumericOrderIdRejected);
                    }
                    let numeric = field
                        .as_i64()
                        .filter(|value| *value > 0)
                        .ok_or(Stage5cRuntimeStateRestoreError::InvalidLegacyNumericOrderId)?;
                    let converted =
                        BrokerOrderId::try_from_legacy_alor_numeric(numeric).map_err(|_| {
                            Stage5cRuntimeStateRestoreError::InvalidLegacyNumericOrderId
                        })?;
                    *field = serde_json::Value::String(converted.as_str().to_string());
                } else {
                    normalize_legacy_order_ids(field, policy)?;
                }
            }
            Ok(())
        }
        serde_json::Value::Array(values) => {
            for value in values {
                normalize_legacy_order_ids(value, policy)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_post_bootstrap_broker_truth(
    strategy: &HybridIntradayRuntimeStrategy,
    admission: &Stage5cPaperHostAdmission,
) -> Result<(), Stage5cRuntimeStateRestoreError> {
    let state = Strategy::state(strategy);
    let broker_qty = admission
        .bootstrap_snapshot()
        .target_position_qty
        .to_f64()
        .ok_or(Stage5cRuntimeStateRestoreError::BrokerTruthPositionMismatch)?;
    match state {
        StrategyState::HybridIntradayRuntime {
            last_position_qty,
            current_side,
            tp_order_id,
            sl_stop_order_id,
            sl_exchange_order_id,
            ..
        } => {
            if (*last_position_qty - broker_qty).abs() > f64::EPSILON {
                return Err(Stage5cRuntimeStateRestoreError::BrokerTruthPositionMismatch);
            }
            let expected = if broker_qty > f64::EPSILON {
                Some(crate::hybrid_intraday::Side::Long)
            } else if broker_qty < -f64::EPSILON {
                Some(crate::hybrid_intraday::Side::Short)
            } else {
                None
            };
            if expected.is_some() && *current_side != expected {
                return Err(Stage5cRuntimeStateRestoreError::BrokerTruthSideMismatch);
            }
            if tp_order_id.is_some() || sl_stop_order_id.is_some() || sl_exchange_order_id.is_some()
            {
                return Err(Stage5cRuntimeStateRestoreError::BrokerOwnedOrderIdMismatch);
            }
            Ok(())
        }
        StrategyState::Idle => Err(Stage5cRuntimeStateRestoreError::WrongStrategyStateKind),
    }
}

pub fn accept_stage5c_history_batch(
    input: Stage5cHistoryBatchInput,
) -> Result<Stage5cAcceptedHistoryBatch, Stage5cHistoryWarmupError> {
    if input.bars.is_empty() {
        return Err(Stage5cHistoryWarmupError::EmptyHistory);
    }
    let instrument = input.bars[0].instrument.clone();
    let mut previous_close_ts = None;
    for bar in &input.bars {
        if bar.instrument != instrument {
            return Err(Stage5cHistoryWarmupError::InstrumentMismatch);
        }
        if bar.timeframe_sec != 600 {
            return Err(Stage5cHistoryWarmupError::InvalidTimeframe);
        }
        if !bar.is_final {
            return Err(Stage5cHistoryWarmupError::NonFinalBar);
        }
        if bar.origin != broker_core::HybridRuntimeBarOrigin::History {
            return Err(Stage5cHistoryWarmupError::InvalidOrigin);
        }
        if bar.close_time_utc.rem_euclid(600) != 0 {
            return Err(Stage5cHistoryWarmupError::UnalignedTimestamp);
        }
        let close_ts = DateTime::<Utc>::from_timestamp(bar.close_time_utc, 0)
            .ok_or(Stage5cHistoryWarmupError::InvalidHistoryTimestamp)?;
        let open_ts = close_ts
            .checked_sub_signed(chrono::Duration::seconds(600))
            .ok_or(Stage5cHistoryWarmupError::InvalidHistoryTimestamp)?;
        if previous_close_ts.is_some_and(|previous| bar.close_time_utc <= previous) {
            return Err(Stage5cHistoryWarmupError::NonMonotonicTimestamp);
        }
        previous_close_ts = Some(bar.close_time_utc);
        if ![bar.open, bar.high, bar.low, bar.close]
            .iter()
            .all(|value| value.is_finite())
            || bar.low > bar.high
            || bar.high < bar.open.max(bar.close)
            || bar.low > bar.open.min(bar.close)
        {
            return Err(Stage5cHistoryWarmupError::InvalidOhlc);
        }
        if !bar.volume.is_finite() || bar.volume < 0.0 {
            return Err(Stage5cHistoryWarmupError::InvalidVolume);
        }
        let stage3_bar = broker_core::event::Bar {
            instrument: bar.instrument.clone(),
            source_kind: broker_core::MarketDataSourceKind::HistoricalPoll,
            timeframe_sec: bar.timeframe_sec,
            open_ts,
            close_ts,
            open: rust_decimal::Decimal::ZERO,
            high: rust_decimal::Decimal::ZERO,
            low: rust_decimal::Decimal::ZERO,
            close: rust_decimal::Decimal::ZERO,
            volume: rust_decimal::Decimal::ZERO,
            is_final: bar.is_final,
        };
        if !broker_core::evaluate_stage3_strategy_input_gate(&stage3_bar, &input.provenance)
            .accepted
        {
            return Err(Stage5cHistoryWarmupError::Stage3ProvenanceRejected);
        }
    }
    Ok(Stage5cAcceptedHistoryBatch {
        start_ts: input.bars[0].close_time_utc,
        end_ts: input.bars.last().expect("non-empty history").close_time_utc,
        bars: input.bars,
        provenance: input.provenance,
        instrument,
    })
}

pub fn warmup_stage5c_history(
    restored: Stage5cRuntimeStateRestoredPaperStrategy,
    history: Stage5cAcceptedHistoryBatch,
) -> Result<Stage5cWarmedPaperStrategy, Stage5cHistoryWarmupError> {
    warmup_stage5c_history_at(restored, history, Utc::now())
}

fn warmup_stage5c_history_at(
    restored: Stage5cRuntimeStateRestoredPaperStrategy,
    history: Stage5cAcceptedHistoryBatch,
    warmup_now: DateTime<Utc>,
) -> Result<Stage5cWarmedPaperStrategy, Stage5cHistoryWarmupError> {
    let (mut strategy, restore_receipt) = restored.into_parts();
    let bootstrap_receipt = restore_receipt.bootstrap_receipt();
    let admission = &bootstrap_receipt.admission;
    if warmup_now > bootstrap_receipt.expires_at() {
        return Err(Stage5cHistoryWarmupError::BrokerTruthExpired);
    }
    if !(admission.checked_ts() <= admission.issued_ts()
        && admission.issued_ts() <= bootstrap_receipt.notified_ts()
        && bootstrap_receipt.notified_ts() <= restore_receipt.restored_ts()
        && restore_receipt.restored_ts() <= warmup_now)
    {
        return Err(Stage5cHistoryWarmupError::LifecycleTimestampReversal);
    }
    validate_stage5cd_time_boundary(&history, admission, warmup_now)?;

    let input_bars = history.bars.len();
    let source_mode = history.provenance.source_mode;
    let last_history_ts = history.end_ts;
    let mut bars = Vec::with_capacity(input_bars);
    for bar in history.bars {
        bars.push(crate::runtime_compat::BarEvent {
            symbol: bar.instrument.symbol,
            close_time_utc: bar.close_time_utc,
            close: bar.close,
            o: bar.open,
            h: bar.high,
            l: bar.low,
            v: bar.volume,
            origin: crate::runtime_compat::DataOrigin::History,
        });
    }

    let context = StrategyCtx {
        strategy_id: admission.strategy_id().to_string(),
        portfolio: admission.account_id().as_str().to_string(),
        exchange: format!("{:?}", admission.target_instrument().exchange),
        symbol: admission.target_instrument().symbol.clone(),
        tick_size: admission.tick_size(),
        trade_mode: TradeMode::Paper,
        paper_execution_mode: PaperExecutionMode::HistorySim,
        allow_live_orders: false,
        gateway_phase: GatewayPhase::SyncingHistory,
        position_qty: admission.bootstrap_snapshot().target_position_qty.to_f64(),
        event_ts_utc: bars
            .last()
            .map_or(warmup_now.timestamp(), |bar| bar.close_time_utc),
        now_ts_utc: warmup_now.timestamp(),
        last_bar_ts: bars.last().map(|bar| bar.close_time_utc),
    };
    let processed_bars = Strategy::warmup_from_history(&mut strategy, &context, &bars);
    if processed_bars == 0 {
        return Err(Stage5cHistoryWarmupError::NoEligibleHistoryBars);
    }

    Ok(Stage5cWarmedPaperStrategy {
        strategy,
        receipt: Stage5cHistoryWarmupReceipt {
            restore_receipt,
            started_ts: warmup_now,
            processed_bars,
            input_bars,
            source_mode,
            last_history_ts,
        },
    })
}

fn validate_stage5cd_time_boundary(
    history: &Stage5cAcceptedHistoryBatch,
    admission: &Stage5cPaperHostAdmission,
    warmup_now: DateTime<Utc>,
) -> Result<(), Stage5cHistoryWarmupError> {
    if history.instrument != *admission.target_instrument() {
        return Err(Stage5cHistoryWarmupError::InstrumentMismatch);
    }
    if DateTime::<Utc>::from_timestamp(history.start_ts, 0).is_none()
        || DateTime::<Utc>::from_timestamp(history.end_ts, 0).is_none()
    {
        return Err(Stage5cHistoryWarmupError::InvalidHistoryTimestamp);
    }
    if history.end_ts > warmup_now.timestamp() {
        return Err(Stage5cHistoryWarmupError::FutureHistoryBar);
    }
    Ok(())
}

pub fn prove_stage5c_pending_recovery_claim(
    warmed: &Stage5cWarmedPaperStrategy,
    input: Stage5cPendingRecoveryClaimProofInput,
) -> Result<Stage5cPendingRecoveryClaimProof, Stage5cPendingRecoveryError> {
    let admission = &warmed
        .receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    if input.strategy_id != admission.strategy_id()
        || input.account_id != *admission.account_id()
        || input.target_instrument != *admission.target_instrument()
        || input.snapshot_received_ts != admission.bootstrap_snapshot().received_ts
    {
        return Err(Stage5cPendingRecoveryError::ClaimScopeMismatch);
    }
    let required = [
        Stage5cPendingStreamKind::Ack,
        Stage5cPendingStreamKind::Order,
        Stage5cPendingStreamKind::StopOrder,
        Stage5cPendingStreamKind::Position,
    ];
    if input.completed_ts < warmed.receipt().started_ts()
        || input.streams.len() != required.len()
        || required.iter().any(|kind| {
            !input
                .streams
                .iter()
                .any(|stream| stream.stream_kind == *kind)
        })
        || input.streams.iter().any(|stream| {
            stream.stream_name
                != canonical_pending_stream_name(stream.stream_kind, &input.account_id)
                || stream.consumer_group
                    != format!("paper-runtime:{}:{}", input.account_id, input.strategy_id)
                || stream.terminal_claim_cursor != "0-0"
                || parse_redis_stream_id(&stream.snapshot_boundary_entry_id).is_none()
        })
    {
        return Err(Stage5cPendingRecoveryError::ClaimBoundaryInvalid);
    }
    Ok(Stage5cPendingRecoveryClaimProof {
        strategy_id: input.strategy_id,
        account_id: input.account_id,
        target_instrument: input.target_instrument,
        snapshot_received_ts: input.snapshot_received_ts,
        completed_ts: input.completed_ts,
        streams: input.streams,
    })
}

pub fn accept_stage5c_pending_recovery_evidence(
    input: Stage5cPendingRecoveryEvidenceInput,
) -> Result<Stage5cAcceptedPendingRecoveryEvidence, Stage5cPendingRecoveryError> {
    let mut unique = HashMap::<(String, String), Stage5cPendingRecoveryEvent>::new();
    let mut duplicate_events = 0usize;
    for event in input.events {
        if event.stream_name.trim().is_empty() || parse_redis_stream_id(&event.entry_id).is_none() {
            return Err(Stage5cPendingRecoveryError::InvalidEventIdentity);
        }
        let boundary = input
            .claim_proof
            .streams
            .iter()
            .find(|stream| {
                stream.stream_kind == event.stream_kind && stream.stream_name == event.stream_name
            })
            .ok_or(Stage5cPendingRecoveryError::StreamKindMismatch)?;
        let payload_kind = match &event.payload {
            Stage5cPendingRecoveryPayload::Ack(_) => Stage5cPendingStreamKind::Ack,
            Stage5cPendingRecoveryPayload::Order(_) => Stage5cPendingStreamKind::Order,
            Stage5cPendingRecoveryPayload::StopOrder(_) => Stage5cPendingStreamKind::StopOrder,
            Stage5cPendingRecoveryPayload::Position(_) => Stage5cPendingStreamKind::Position,
        };
        if payload_kind != event.stream_kind || boundary.consumer_group.trim().is_empty() {
            return Err(Stage5cPendingRecoveryError::StreamKindMismatch);
        }
        let key = (event.stream_name.clone(), event.entry_id.clone());
        if let Some(existing) = unique.get(&key) {
            if existing != &event {
                return Err(Stage5cPendingRecoveryError::ConflictingDuplicate);
            }
            duplicate_events += 1;
        } else {
            unique.insert(key, event);
        }
    }
    let mut events: Vec<_> = unique.into_values().collect();
    events.sort_by_key(|event| event.sequence);
    if events
        .windows(2)
        .any(|pair| pair[0].sequence >= pair[1].sequence)
    {
        return Err(Stage5cPendingRecoveryError::NonMonotonicSequence);
    }
    if input.claim_proof.streams.iter().any(|stream| {
        stream.claimed_count
            != events
                .iter()
                .filter(|event| event.stream_kind == stream.stream_kind)
                .count()
    }) {
        return Err(Stage5cPendingRecoveryError::ClaimBoundaryInvalid);
    }
    Ok(Stage5cAcceptedPendingRecoveryEvidence {
        events,
        duplicate_events,
        claim_proof: input.claim_proof,
    })
}

fn canonical_pending_stream_name(
    kind: Stage5cPendingStreamKind,
    account: &BrokerAccountId,
) -> String {
    let prefix = match kind {
        Stage5cPendingStreamKind::Ack => "cmd.acks",
        Stage5cPendingStreamKind::Order => "broker.orders",
        Stage5cPendingStreamKind::StopOrder => "broker.stop_orders",
        Stage5cPendingStreamKind::Position => "broker.positions",
    };
    format!("{prefix}.{account}")
}

fn parse_redis_stream_id(value: &str) -> Option<(u64, u64)> {
    let (milliseconds, sequence) = value.split_once('-')?;
    Some((milliseconds.parse().ok()?, sequence.parse().ok()?))
}

pub fn recover_stage5c_pending_streams(
    warmed: Stage5cWarmedPaperStrategy,
    evidence: Stage5cAcceptedPendingRecoveryEvidence,
) -> Result<Stage5cPendingRecoveredPaperStrategy, Stage5cPendingRecoveryError> {
    recover_stage5c_pending_streams_at(warmed, evidence, Utc::now())
}

fn recover_stage5c_pending_streams_at(
    warmed: Stage5cWarmedPaperStrategy,
    evidence: Stage5cAcceptedPendingRecoveryEvidence,
    recovered_ts: DateTime<Utc>,
) -> Result<Stage5cPendingRecoveredPaperStrategy, Stage5cPendingRecoveryError> {
    let (mut strategy, warmup_receipt) = warmed.into_parts();
    let bootstrap_receipt = warmup_receipt.restore_receipt().bootstrap_receipt();
    let admission = &bootstrap_receipt.admission;
    if recovered_ts > bootstrap_receipt.expires_at() {
        return Err(Stage5cPendingRecoveryError::BrokerTruthExpired);
    }
    if warmup_receipt.started_ts() > recovered_ts {
        return Err(Stage5cPendingRecoveryError::LifecycleTimestampReversal);
    }
    if evidence.claim_proof.strategy_id != admission.strategy_id()
        || evidence.claim_proof.account_id != *admission.account_id()
        || evidence.claim_proof.target_instrument != *admission.target_instrument()
        || evidence.claim_proof.snapshot_received_ts != admission.bootstrap_snapshot().received_ts
        || evidence.claim_proof.completed_ts > recovered_ts
    {
        return Err(Stage5cPendingRecoveryError::ClaimScopeMismatch);
    }
    for event in &evidence.events {
        let instrument = match &event.payload {
            Stage5cPendingRecoveryPayload::Ack(_) => None,
            Stage5cPendingRecoveryPayload::Order(value) => Some(&value.instrument),
            Stage5cPendingRecoveryPayload::StopOrder(value) => Some(&value.instrument),
            Stage5cPendingRecoveryPayload::Position(value) => Some(&value.instrument),
        };
        if instrument.is_some_and(|value| value != admission.target_instrument()) {
            return Err(Stage5cPendingRecoveryError::InstrumentMismatch);
        }
        let event_ts = match &event.payload {
            Stage5cPendingRecoveryPayload::Ack(value) => value.processed_ts_utc,
            Stage5cPendingRecoveryPayload::Order(value) => value.source_ts_utc,
            Stage5cPendingRecoveryPayload::StopOrder(value) => value.source_ts_utc,
            Stage5cPendingRecoveryPayload::Position(value) => value.source_ts_utc,
        };
        if DateTime::<Utc>::from_timestamp(event_ts, 0).is_none() {
            return Err(Stage5cPendingRecoveryError::InvalidEventTimestamp);
        }
        if event_ts > recovered_ts.timestamp() {
            return Err(Stage5cPendingRecoveryError::FutureEvent);
        }
        if let Stage5cPendingRecoveryPayload::Ack(value) = &event.payload {
            if !warmup_receipt
                .restore_receipt()
                .pending_requests()
                .contains(&value.request_id)
            {
                return Err(Stage5cPendingRecoveryError::AckNotPending);
            }
        }
    }
    let position_qty = admission.bootstrap_snapshot().target_position_qty.to_f64();
    let mut replayed_events = 0usize;
    let duplicate_events = evidence.duplicate_events;
    for event in evidence.events {
        let event_ts = match &event.payload {
            Stage5cPendingRecoveryPayload::Ack(value) => value.processed_ts_utc,
            Stage5cPendingRecoveryPayload::Order(value) => value.source_ts_utc,
            Stage5cPendingRecoveryPayload::StopOrder(value) => value.source_ts_utc,
            Stage5cPendingRecoveryPayload::Position(value) => value.source_ts_utc,
        };
        let context = broker_core::HybridRuntimeStrategyContext {
            strategy_id: admission.strategy_id().to_string(),
            request_namespace_account: admission.account_id().clone(),
            instrument: admission.target_instrument().clone(),
            tick_size: admission.tick_size(),
            trade_mode: broker_core::HybridRuntimeTradeMode::Paper,
            paper_execution_mode: broker_core::HybridRuntimePaperExecutionMode::LiveOnly,
            allow_live_orders: false,
            gateway_phase: broker_core::HybridRuntimeGatewayPhase::CatchingUp,
            position_qty,
            event_ts_utc: event_ts,
            strategy_now_ts_utc: recovered_ts.timestamp(),
            last_bar_ts_utc: None,
        };
        let boundary = evidence
            .claim_proof
            .streams
            .iter()
            .find(|stream| stream.stream_kind == event.stream_kind)
            .expect("accepted evidence has every typed stream");
        let entry_id =
            parse_redis_stream_id(&event.entry_id).expect("accepted evidence has valid entry IDs");
        let snapshot_boundary = parse_redis_stream_id(&boundary.snapshot_boundary_entry_id)
            .expect("accepted proof has valid boundary IDs");
        if !matches!(&event.payload, Stage5cPendingRecoveryPayload::Ack(_))
            && entry_id <= snapshot_boundary
        {
            continue;
        }
        let result = match event.payload {
            Stage5cPendingRecoveryPayload::Ack(value) => {
                crate::BrokerNeutralHybridStrategy::on_broker_ack(&mut strategy, value)
            }
            Stage5cPendingRecoveryPayload::Order(value) => {
                crate::BrokerNeutralHybridStrategy::on_broker_order(
                    &mut strategy,
                    broker_core::HybridRuntimeCallbackInput {
                        context,
                        payload: value,
                    },
                )
            }
            Stage5cPendingRecoveryPayload::StopOrder(value) => {
                crate::BrokerNeutralHybridStrategy::on_broker_stop_order(
                    &mut strategy,
                    broker_core::HybridRuntimeCallbackInput {
                        context,
                        payload: value,
                    },
                )
            }
            Stage5cPendingRecoveryPayload::Position(value) => {
                crate::BrokerNeutralHybridStrategy::on_broker_position(
                    &mut strategy,
                    broker_core::HybridRuntimeCallbackInput {
                        context,
                        payload: value,
                    },
                )
            }
        }
        .map_err(|_| Stage5cPendingRecoveryError::CallbackValidationFailed)?;
        if !result.is_empty() {
            return Err(Stage5cPendingRecoveryError::UnexpectedIntent);
        }
        replayed_events += 1;
    }
    Ok(Stage5cPendingRecoveredPaperStrategy {
        strategy,
        receipt: Stage5cPendingRecoveryReceipt {
            warmup_receipt,
            recovered_ts,
            replayed_events,
            duplicate_events,
        },
    })
}

pub fn accept_stage5c_semantic_bar(
    input: Stage5cSemanticBarInput,
) -> Result<Stage5cAcceptedSemanticBar, Stage5cSemanticBarError> {
    let close_ts = DateTime::<Utc>::from_timestamp(input.bar.close_time_utc, 0)
        .ok_or(Stage5cSemanticBarError::InvalidTimestamp)?;
    let open_ts = close_ts
        .checked_sub_signed(chrono::Duration::seconds(600))
        .ok_or(Stage5cSemanticBarError::InvalidTimestamp)?;
    if !matches!(
        input.bar.origin,
        broker_core::HybridRuntimeBarOrigin::Live | broker_core::HybridRuntimeBarOrigin::Replay
    ) {
        return Err(Stage5cSemanticBarError::Stage3Rejected);
    }
    if input.bar.close_time_utc.rem_euclid(600) != 0 {
        return Err(Stage5cSemanticBarError::UnalignedTimestamp);
    }
    if ![
        input.bar.open,
        input.bar.high,
        input.bar.low,
        input.bar.close,
    ]
    .iter()
    .all(|value| value.is_finite())
        || input.bar.low > input.bar.high
        || input.bar.high < input.bar.open.max(input.bar.close)
        || input.bar.low > input.bar.open.min(input.bar.close)
    {
        return Err(Stage5cSemanticBarError::InvalidOhlc);
    }
    if !input.bar.volume.is_finite() || input.bar.volume < 0.0 {
        return Err(Stage5cSemanticBarError::InvalidVolume);
    }
    let gate_bar = broker_core::event::Bar {
        instrument: input.bar.instrument.clone(),
        source_kind: broker_core::MarketDataSourceKind::LiveStream,
        timeframe_sec: input.bar.timeframe_sec,
        open_ts,
        close_ts,
        open: rust_decimal::Decimal::ZERO,
        high: rust_decimal::Decimal::ZERO,
        low: rust_decimal::Decimal::ZERO,
        close: rust_decimal::Decimal::ZERO,
        volume: rust_decimal::Decimal::ZERO,
        is_final: input.bar.is_final,
    };
    if !broker_core::evaluate_stage3_strategy_input_gate(&gate_bar, &input.provenance).accepted {
        return Err(Stage5cSemanticBarError::Stage3Rejected);
    }
    // STAGE5D-ADDITIVE-BRIDGE-BEGIN: stage5e-b3c-semantic-identity-admission
    let stage3_provenance_identity = stage5e_b3c_stage3_provenance_identity(&input.provenance);
    let semantic_bar_identity =
        stage5e_b3c_semantic_bar_identity(&input.bar, stage3_provenance_identity);
    // STAGE5D-ADDITIVE-BRIDGE-END: stage5e-b3c-semantic-identity-admission
    Ok(Stage5cAcceptedSemanticBar {
        origin: input.bar.origin,
        bar: input.bar,
        tick_size: input.tick_size,
        // STAGE5D-ADDITIVE-BRIDGE-BEGIN: stage5e-b3c-semantic-identity-construction
        stage3_provenance_identity,
        semantic_bar_identity,
        // STAGE5D-ADDITIVE-BRIDGE-END: stage5e-b3c-semantic-identity-construction
    })
}

pub fn apply_stage5c_semantic_bar(
    recovered: Stage5cPendingRecoveredPaperStrategy,
    accepted: Stage5cAcceptedSemanticBar,
) -> Result<Stage5cSemanticBarResult, Stage5cSemanticBarError> {
    apply_stage5c_semantic_bar_at(recovered, accepted, Utc::now())
}
// STAGE5D-ADDITIVE-BRIDGE-BEGIN: stage5e-b3e-callback-materialization
// STAGE5E-B3E-CALLBACK-IMPLEMENTATION-BEGIN: private-materialization-v1
pub(crate) struct Stage5cB3eCallbackMaterialSeal(());

pub(crate) fn issue_stage5c_b3e_callback_material_seal(
    _nested_consume_capability: &crate::stage5e_no_io_lifecycle::callback_authority::Stage5eB3eNestedConsumeSeal,
) -> Stage5cB3eCallbackMaterialSeal {
    Stage5cB3eCallbackMaterialSeal(())
}

pub(crate) struct Stage5eStage5cMaterializationTerminalBlock(());

fn construct_stage5e_stage5c_materialization_terminal_block(
) -> Stage5eStage5cMaterializationTerminalBlock {
    Stage5eStage5cMaterializationTerminalBlock(())
}

#[allow(dead_code)] // Opaque ownership is inspected only by a later settlement stage.
pub(crate) struct Stage5ePreCallbackAttributionSnapshot {
    cleanup_ledger: Stage5cCleanupAttributionLedger,
    strategy_id: String,
    account_id: BrokerAccountId,
    target_instrument: InstrumentId,
    accepted_semantic_bar_identity: [u8; 32],
    accepted_bar_close_ts: i64,
}

#[allow(dead_code)] // Opaque ownership is inspected only by a later settlement stage.
pub(crate) struct Stage5eAcceptedBarSettlementMetadata {
    accepted_bar_close_ts: i64,
    accepted_bar_origin: broker_core::HybridRuntimeBarOrigin,
    execution_eligible: bool,
    accepted_semantic_bar_identity: [u8; 32],
}

#[cfg(test)]
impl Stage5ePreCallbackAttributionSnapshot {
    pub(crate) fn test_corrupt_strategy_id(&mut self) {
        self.strategy_id.push_str("_MISMATCH");
    }

    pub(crate) fn test_corrupt_account_id(&mut self) {
        self.account_id = BrokerAccountId::new("ACC_TEST_MISMATCH");
    }

    pub(crate) fn test_corrupt_target_instrument(&mut self) {
        self.target_instrument.symbol.push_str("_MISMATCH");
    }

    pub(crate) fn test_corrupt_semantic_bar_identity(&mut self) {
        self.accepted_semantic_bar_identity[0] ^= 1;
    }

    pub(crate) fn test_set_accepted_bar_close_ts(&mut self, accepted_bar_close_ts: i64) {
        self.accepted_bar_close_ts = accepted_bar_close_ts;
    }

    pub(crate) fn test_ownership_shape(&self) -> (usize, usize, bool) {
        (
            self.cleanup_ledger.broker_orders.len(),
            self.cleanup_ledger.stop_orders.len(),
            self.cleanup_ledger.pending_entry_attribution.is_some(),
        )
    }

    pub(crate) fn test_binding_vector(
        &self,
    ) -> (String, BrokerAccountId, InstrumentId, [u8; 32], i64) {
        (
            self.strategy_id.clone(),
            self.account_id.clone(),
            self.target_instrument.clone(),
            self.accepted_semantic_bar_identity,
            self.accepted_bar_close_ts,
        )
    }
}

#[cfg(test)]
impl Stage5eAcceptedBarSettlementMetadata {
    pub(crate) fn test_corrupt_accepted_bar_origin(&mut self) {
        self.accepted_bar_origin = broker_core::HybridRuntimeBarOrigin::Replay;
    }

    pub(crate) fn test_disable_execution_eligibility(&mut self) {
        self.execution_eligible = false;
    }

    pub(crate) fn test_set_accepted_bar_close_ts(&mut self, accepted_bar_close_ts: i64) {
        self.accepted_bar_close_ts = accepted_bar_close_ts;
    }

    pub(crate) fn test_retained_bar_metadata(
        &self,
    ) -> (i64, broker_core::HybridRuntimeBarOrigin, bool, [u8; 32]) {
        (
            self.accepted_bar_close_ts,
            self.accepted_bar_origin,
            self.execution_eligible,
            self.accepted_semantic_bar_identity,
        )
    }
}

#[cfg(test)]
impl Stage5cPendingRecoveryReceipt {
    pub(crate) fn test_disable_paper_mode(&mut self) {
        self.warmup_receipt
            .restore_receipt
            .bootstrap_receipt
            .admission
            .paper_only = false;
    }
}

pub(crate) struct Stage5eStage5cAuthorizedCallbackMaterial {
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    callback_input: broker_core::HybridRuntimeCallbackInput<broker_core::HybridRuntimeBarEvent>,
    attribution_snapshot: Stage5ePreCallbackAttributionSnapshot,
    retained_bar_metadata: Stage5eAcceptedBarSettlementMetadata,
}

pub(crate) struct Stage5eStage5cPostCallbackMaterial {
    mutated_strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    attribution_snapshot: Stage5ePreCallbackAttributionSnapshot,
    retained_bar_metadata: Stage5eAcceptedBarSettlementMetadata,
    callback_outcome:
        crate::stage5e_no_io_lifecycle::callback_authority::Stage5ePaperCallbackOutcome,
}

#[cfg(test)]
thread_local! {
    static STAGE5E_B3E_CALLBACK_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn stage5e_test_reset_b3e_callback_count() {
    STAGE5E_B3E_CALLBACK_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
pub(crate) fn stage5e_test_b3e_callback_count() -> usize {
    STAGE5E_B3E_CALLBACK_COUNT.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(crate) fn stage5e_test_nonempty_intent_sequence_inputs(
    now: DateTime<Utc>,
    predecessor_close_ts: i64,
    current_close_ts: i64,
) -> (
    Stage5cPendingRecoveredPaperStrategy,
    Stage5cAcceptedSemanticBar,
) {
    let target = InstrumentId {
        symbol: "IMOEXF".to_string(),
        venue_symbol: Some("IMOEXF@RTSX".to_string()),
        exchange: broker_core::Exchange::Moex,
        market: broker_core::Market::Futures,
    };
    let mut strategy = HybridIntradayRuntimeStrategy::new(
        crate::hybrid_intraday_runtime::HybridIntradayRuntimeConfig {
            symbol: "IMOEXF".to_string(),
            profile: crate::hybrid_intraday_runtime::HybridIntradayProfile::BaselineRuntimeHybrid,
            mr_variant: crate::hybrid_intraday_runtime::MeanReversionVariant::Author41BoundaryShort,
            mr_gate_policy: crate::hybrid_intraday_runtime::MrGatePolicy::Disabled,
            risk_gate_mode: crate::hybrid_intraday_runtime::RiskGateMode::Disabled,
            risk_gate_seed_file: None,
            risk_gate_ledger_key: None,
            model_session_start_time: None,
            model_session_end_time: None,
            qty: 1.0,
            live_order_style: crate::runtime_compat::MarketBuyAndCloseLiveOrderStyle::Market,
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
            pending_timeout_sec: 30,
            partial_entry_fill_timeout_ms: 3_000,
            mr_config: crate::hybrid_intraday::MeanReversionConfig::default(),
            breakout_config: crate::hybrid_intraday::IntradayBreakoutConfig::default(),
            orchestrator_config: crate::hybrid_intraday::HybridOrchestratorConfig::default(),
        },
    );
    for (close_time_utc, high, low) in [
        (current_close_ts - 86_400 - 600, 2630.0, 2570.0),
        (current_close_ts - 86_400, 2620.0, 2580.0),
    ] {
        let history_result = Strategy::on_bar(
            &mut strategy,
            &StrategyCtx {
                strategy_id: "stage5e_test".to_string(),
                portfolio: "ACC_TEST_0001".to_string(),
                exchange: "MOEX".to_string(),
                symbol: "IMOEXF".to_string(),
                tick_size: 0.5,
                trade_mode: TradeMode::Paper,
                paper_execution_mode: PaperExecutionMode::LiveOnly,
                allow_live_orders: false,
                gateway_phase: GatewayPhase::LiveReady,
                position_qty: Some(0.0),
                event_ts_utc: close_time_utc,
                now_ts_utc: close_time_utc,
                last_bar_ts: Some(close_time_utc),
            },
            &crate::runtime_compat::BarEvent {
                symbol: "IMOEXF".to_string(),
                close_time_utc,
                o: 2600.0,
                h: high,
                l: low,
                close: 2600.0,
                v: 1.0,
                origin: crate::runtime_compat::DataOrigin::Replay,
            },
        );
        assert!(
            history_result.is_empty(),
            "replay warmup must not emit executable intents"
        );
    }

    let admission = Stage5cPaperHostAdmission::stage5d_test_new(
        "stage5e_test".to_string(),
        BrokerAccountId::new("ACC_TEST_0001"),
        target.clone(),
        0.5,
        rust_decimal::Decimal::ZERO,
        now,
    );
    let recovery_receipt = Stage5cPendingRecoveryReceipt {
        warmup_receipt: Stage5cHistoryWarmupReceipt {
            restore_receipt: Stage5cRuntimeStateRestoreReceipt {
                bootstrap_receipt: Stage5cBootstrapNotificationReceipt {
                    admission,
                    notified_ts: now,
                },
                restored_ts: now,
                known_order_ids: Vec::new(),
                pending_requests: Vec::new(),
            },
            started_ts: now,
            processed_bars: 2,
            input_bars: 2,
            source_mode: broker_core::Stage3StrategyBarSourceMode::FinamDerivedM1ToM10,
            last_history_ts: predecessor_close_ts,
        },
        recovered_ts: now,
        replayed_events: 0,
        duplicate_events: 0,
    };
    let bar = broker_core::HybridRuntimeBarEvent {
        instrument: target,
        close_time_utc: current_close_ts,
        open: 2601.0,
        high: 2602.0,
        low: 2599.0,
        close: 2601.0,
        volume: 1.0,
        origin: broker_core::HybridRuntimeBarOrigin::Live,
        is_final: true,
        timeframe_sec: 600,
    };
    let provenance = broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete();
    let stage3_provenance_identity = stage5e_b3c_stage3_provenance_identity(&provenance);
    let semantic_bar_identity = stage5e_b3c_semantic_bar_identity(&bar, stage3_provenance_identity);
    (
        Stage5cPendingRecoveredPaperStrategy {
            strategy,
            receipt: recovery_receipt,
        },
        Stage5cAcceptedSemanticBar {
            bar,
            tick_size: 0.5,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            stage3_provenance_identity,
            semantic_bar_identity,
        },
    )
}

pub(crate) fn consume_stage5c_for_authorized_callback(
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    accepted: Stage5cAcceptedSemanticBar,
    _seal: Stage5cB3eCallbackMaterialSeal,
    callback_now: DateTime<Utc>,
) -> Result<Stage5eStage5cAuthorizedCallbackMaterial, Stage5eStage5cMaterializationTerminalBlock> {
    let admission = &recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    if accepted.bar.instrument != *admission.target_instrument()
        || !same_tick_size(accepted.tick_size, admission.tick_size())
    {
        return Err(construct_stage5e_stage5c_materialization_terminal_block());
    }

    let attribution_snapshot = Stage5ePreCallbackAttributionSnapshot {
        cleanup_ledger: stage5cj_cleanup_attribution_ledger(
            Strategy::state(&strategy),
            admission.strategy_id(),
        ),
        strategy_id: admission.strategy_id().to_string(),
        account_id: admission.account_id().clone(),
        target_instrument: admission.target_instrument().clone(),
        accepted_semantic_bar_identity: accepted.semantic_bar_identity,
        accepted_bar_close_ts: accepted.bar.close_time_utc,
    };
    let retained_bar_metadata = Stage5eAcceptedBarSettlementMetadata {
        accepted_bar_close_ts: accepted.bar.close_time_utc,
        accepted_bar_origin: accepted.origin,
        execution_eligible: accepted.origin == broker_core::HybridRuntimeBarOrigin::Live,
        accepted_semantic_bar_identity: accepted.semantic_bar_identity,
    };
    let context = stage5cf_semantic_context(
        &strategy,
        admission,
        accepted.bar.close_time_utc,
        callback_now,
    );
    Ok(Stage5eStage5cAuthorizedCallbackMaterial {
        strategy,
        recovery_receipt,
        callback_input: broker_core::HybridRuntimeCallbackInput {
            context,
            payload: accepted.bar,
        },
        attribution_snapshot,
        retained_bar_metadata,
    })
}

impl Stage5eStage5cAuthorizedCallbackMaterial {
    pub(crate) fn invoke_authorized_callback_once(
        self,
        execution_capability: crate::stage5e_no_io_lifecycle::callback_authority::Stage5cB3eCallbackExecutionSeal,
    ) -> Stage5eStage5cPostCallbackMaterial {
        let Self {
            mut strategy,
            recovery_receipt,
            callback_input,
            attribution_snapshot,
            retained_bar_metadata,
        } = self;
        #[cfg(test)]
        STAGE5E_B3E_CALLBACK_COUNT.with(|count| count.set(count.get() + 1));
        let exact_result =
            crate::BrokerNeutralHybridStrategy::on_broker_bar(&mut strategy, callback_input);
        let callback_outcome =
            crate::stage5e_no_io_lifecycle::callback_authority::move_stage5e_paper_callback_outcome(
                exact_result,
                &execution_capability,
            );
        Stage5eStage5cPostCallbackMaterial {
            mutated_strategy: strategy,
            recovery_receipt,
            attribution_snapshot,
            retained_bar_metadata,
            callback_outcome,
        }
    }
}

impl Stage5eStage5cPostCallbackMaterial {
    pub(crate) fn construct_result_escrow(
        self,
        audit_lineage: crate::stage5e_no_io_lifecycle::callback_authority::Stage5eAuthorizedCallbackAuditLineage,
        callback_invoked_at: DateTime<Utc>,
        callback_authority_id: [u8; 32],
        seal: crate::stage5e_no_io_lifecycle::callback_authority::Stage5eEscrowConstructionSeal,
    ) -> crate::stage5e_no_io_lifecycle::callback_authority::Stage5ePaperCallbackResultEscrow {
        let Self {
            mutated_strategy,
            recovery_receipt,
            attribution_snapshot,
            retained_bar_metadata,
            callback_outcome,
        } = self;
        crate::stage5e_no_io_lifecycle::callback_authority::construct_stage5e_paper_callback_result_escrow(
            mutated_strategy,
            recovery_receipt,
            audit_lineage,
            attribution_snapshot,
            retained_bar_metadata,
            callback_invoked_at,
            callback_authority_id,
            callback_outcome,
            seal,
        )
    }
}

// STAGE5E-B3F-SETTLEMENT-IMPLEMENTATION-BEGIN: stage5c-private-bridge-v1
pub(crate) struct Stage5eB3fStage5cExpectedPreflightBinding<'a> {
    audit_schedule_identity_fingerprint: &'a [u8; 32],
    audit_sequence_identity_fingerprint: &'a [u8; 32],
    audit_event_key_fingerprint: &'a [u8; 32],
    audit_b3b_event_key_fingerprint: &'a [u8; 32],
    audit_full_instrument_id: &'a InstrumentId,
    audit_owned_instrument: &'a InstrumentId,
    audit_accepted_semantic_bar_identity: &'a [u8; 32],
    audit_owned_bar_identity: &'a [u8; 32],
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn construct_stage5e_b3f_stage5c_expected_preflight_binding<'a>(
    audit_schedule_identity_fingerprint: &'a [u8; 32],
    audit_sequence_identity_fingerprint: &'a [u8; 32],
    audit_event_key_fingerprint: &'a [u8; 32],
    audit_b3b_event_key_fingerprint: &'a [u8; 32],
    audit_full_instrument_id: &'a InstrumentId,
    audit_owned_instrument: &'a InstrumentId,
    audit_accepted_semantic_bar_identity: &'a [u8; 32],
    audit_owned_bar_identity: &'a [u8; 32],
    _seal: &crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::Stage5ePaperSettlementPreflightSeal,
) -> Stage5eB3fStage5cExpectedPreflightBinding<'a> {
    Stage5eB3fStage5cExpectedPreflightBinding {
        audit_schedule_identity_fingerprint,
        audit_sequence_identity_fingerprint,
        audit_event_key_fingerprint,
        audit_b3b_event_key_fingerprint,
        audit_full_instrument_id,
        audit_owned_instrument,
        audit_accepted_semantic_bar_identity,
        audit_owned_bar_identity,
    }
}

pub(crate) enum Stage5eStage5cPreflightMismatch {
    StrategyId,
    AccountId,
    FullInstrumentId,
    SemanticBarIdentity,
    AcceptedBarClose,
    AuditEventKey,
    PaperMode,
    AcceptedBarOrigin,
    ExecutionEligibility,
}

pub(crate) struct Stage5eStage5cPreflightValidatedProof(());

pub(crate) struct Stage5eStage5cRetainedCloseChronologyProof(());
pub(crate) struct Stage5eStage5cRetainedCloseChronologyMismatch(());

pub(crate) fn validate_stage5e_b3f_retained_close_chronology(
    retained_bar_metadata: &Stage5eAcceptedBarSettlementMetadata,
    authority_issued_at: DateTime<Utc>,
    callback_invoked_at: DateTime<Utc>,
    _seal: &crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::Stage5ePaperSettlementPreflightSeal,
) -> Result<Stage5eStage5cRetainedCloseChronologyProof, Stage5eStage5cRetainedCloseChronologyMismatch>
{
    let Some(accepted_bar_close) =
        DateTime::from_timestamp(retained_bar_metadata.accepted_bar_close_ts, 0)
    else {
        return Err(Stage5eStage5cRetainedCloseChronologyMismatch(()));
    };
    if accepted_bar_close > authority_issued_at || accepted_bar_close > callback_invoked_at {
        return Err(Stage5eStage5cRetainedCloseChronologyMismatch(()));
    }
    Ok(Stage5eStage5cRetainedCloseChronologyProof(()))
}

pub(crate) fn validate_stage5e_b3f_stage5c_preflight_binding(
    recovery_receipt: &Stage5cPendingRecoveryReceipt,
    attribution_snapshot: &Stage5ePreCallbackAttributionSnapshot,
    retained_bar_metadata: &Stage5eAcceptedBarSettlementMetadata,
    expected: &Stage5eB3fStage5cExpectedPreflightBinding<'_>,
    seal: &crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::Stage5ePaperSettlementPreflightSeal,
) -> Result<Stage5eStage5cPreflightValidatedProof, Stage5eStage5cPreflightMismatch> {
    let admission = &recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    if admission.strategy_id() != attribution_snapshot.strategy_id {
        return Err(Stage5eStage5cPreflightMismatch::StrategyId);
    }
    if admission.account_id() != &attribution_snapshot.account_id {
        return Err(Stage5eStage5cPreflightMismatch::AccountId);
    }
    if admission.target_instrument() != &attribution_snapshot.target_instrument
        || admission.target_instrument() != expected.audit_full_instrument_id
        || admission.target_instrument() != expected.audit_owned_instrument
    {
        return Err(Stage5eStage5cPreflightMismatch::FullInstrumentId);
    }
    if attribution_snapshot.accepted_semantic_bar_identity
        != retained_bar_metadata.accepted_semantic_bar_identity
        || &attribution_snapshot.accepted_semantic_bar_identity
            != expected.audit_accepted_semantic_bar_identity
        || &attribution_snapshot.accepted_semantic_bar_identity != expected.audit_owned_bar_identity
    {
        return Err(Stage5eStage5cPreflightMismatch::SemanticBarIdentity);
    }
    if attribution_snapshot.accepted_bar_close_ts != retained_bar_metadata.accepted_bar_close_ts {
        return Err(Stage5eStage5cPreflightMismatch::AcceptedBarClose);
    }
    if crate::stage5e_no_io_lifecycle::schedule_window_evidence::
        validate_stage5e_b3f_b3b_event_key_binding(
            expected.audit_schedule_identity_fingerprint,
            expected.audit_full_instrument_id,
            retained_bar_metadata.accepted_bar_close_ts,
            expected.audit_sequence_identity_fingerprint,
            expected.audit_event_key_fingerprint,
            expected.audit_b3b_event_key_fingerprint,
            seal,
        )
        .is_err()
    {
        return Err(Stage5eStage5cPreflightMismatch::AuditEventKey);
    }
    if !admission.is_paper_only()
        || admission.runtime_host_attached()
        || admission.intent_sink_attached()
    {
        return Err(Stage5eStage5cPreflightMismatch::PaperMode);
    }
    if retained_bar_metadata.accepted_bar_origin != broker_core::HybridRuntimeBarOrigin::Live {
        return Err(Stage5eStage5cPreflightMismatch::AcceptedBarOrigin);
    }
    if !retained_bar_metadata.execution_eligible {
        return Err(Stage5eStage5cPreflightMismatch::ExecutionEligibility);
    }
    Ok(Stage5eStage5cPreflightValidatedProof(()))
}

pub(crate) struct Stage5cB3fSettlementMaterialSeal(());
pub(crate) struct Stage5cB3fSettlementSeal(());
struct Stage5cB3fSuccessProofSeal(());

pub(crate) fn issue_stage5c_b3f_settlement_material_seal(
    _consume_capability: &crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::Stage5ePaperSettlementConsumeSeal,
) -> Stage5cB3fSettlementMaterialSeal {
    Stage5cB3fSettlementMaterialSeal(())
}

pub(crate) fn issue_stage5c_b3f_settlement_seal(
    _consume_capability: &crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::Stage5ePaperSettlementConsumeSeal,
) -> Stage5cB3fSettlementSeal {
    Stage5cB3fSettlementSeal(())
}

pub(crate) struct Stage5eStage5cSettlementMaterial {
    mutated_strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    pre_callback_attribution_snapshot: Stage5ePreCallbackAttributionSnapshot,
    retained_bar_metadata: Stage5eAcceptedBarSettlementMetadata,
    exact_intent_vector: Vec<crate::BrokerNeutralHybridIntent>,
    derived_original_intent_count: usize,
}

pub(crate) fn construct_stage5e_stage5c_settlement_material(
    mutated_strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    pre_callback_attribution_snapshot: Stage5ePreCallbackAttributionSnapshot,
    retained_bar_metadata: Stage5eAcceptedBarSettlementMetadata,
    exact_intent_vector: Vec<crate::BrokerNeutralHybridIntent>,
    _seal: Stage5cB3fSettlementMaterialSeal,
) -> Stage5eStage5cSettlementMaterial {
    let derived_original_intent_count = exact_intent_vector.len();
    Stage5eStage5cSettlementMaterial {
        mutated_strategy,
        recovery_receipt,
        pre_callback_attribution_snapshot,
        retained_bar_metadata,
        exact_intent_vector,
        derived_original_intent_count,
    }
}

pub(crate) struct Stage5eStage5cSettlementSuccess {
    settled: Stage5cSettledPaperStrategy,
}

pub(crate) struct Stage5eStage5cSettlementSuccessProof<'a> {
    strategy_id: &'a str,
    account_id: &'a BrokerAccountId,
    full_instrument_id: &'a InstrumentId,
    accepted_bar_close_timestamp: i64,
    batch_state_fingerprint: &'a str,
    ordered_strategy_request_ids: &'a [StrategyRequestId],
    intent_count_u8: u8,
    settled_batch_history_length: usize,
    canonical_first_batch_summary: &'a Stage5cPaperIntentBatchSummary,
}

pub(crate) struct Stage5eStage5cSettlementTerminalMaterial {
    mutated_strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    pre_callback_attribution_snapshot: Stage5ePreCallbackAttributionSnapshot,
    retained_bar_metadata: Stage5eAcceptedBarSettlementMetadata,
    exact_stage5c_intent_settlement_error: Stage5cIntentSettlementError,
    derived_original_intent_count: usize,
}

impl Stage5eStage5cSettlementSuccess {
    fn borrow_identity_proof(
        &self,
        _seal: &Stage5cB3fSuccessProofSeal,
    ) -> Stage5eStage5cSettlementSuccessProof<'_> {
        let batch = &self.settled.batch;
        let canonical_first_batch_summary = self
            .settled
            .settled_batch_history
            .first()
            .expect("canonical B3F settlement history is never empty");
        debug_assert_eq!(self.settled.settled_batch_history.len(), 1);
        debug_assert_eq!(
            canonical_first_batch_summary,
            &stage5ch_batch_summary(batch)
        );
        Stage5eStage5cSettlementSuccessProof {
            strategy_id: &batch.strategy_id,
            account_id: &batch.account_id,
            full_instrument_id: &batch.instrument,
            accepted_bar_close_timestamp: batch.bar_close_ts,
            batch_state_fingerprint: &batch.state_fingerprint,
            ordered_strategy_request_ids: &batch.request_ids,
            intent_count_u8: u8::try_from(batch.records.len())
                .expect("canonical Stage 5C batch capacity is u8-bounded"),
            settled_batch_history_length: self.settled.settled_batch_history.len(),
            canonical_first_batch_summary,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn construct_stage5e_success_receipt(
        self,
        audit_lineage: crate::stage5e_no_io_lifecycle::callback_authority::Stage5eAuthorizedCallbackAuditLineage,
        callback_invoked_at: DateTime<Utc>,
        callback_authority_id: [u8; 32],
        accepted_semantic_bar_identity: [u8; 32],
        audit_commitment: [u8; 32],
        success_seal: crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::Stage5ePaperSettlementSuccessSeal,
    ) -> crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::Stage5eValidatedPaperSettlementReceipt{
        let settlement_identity = {
            let proof = self.borrow_identity_proof(&Stage5cB3fSuccessProofSeal(()));
            debug_assert_eq!(proof.settled_batch_history_length, 1);
            debug_assert_eq!(
                proof.canonical_first_batch_summary,
                &stage5ch_batch_summary(&self.settled.batch)
            );
            crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::
                construct_stage5e_b3f_settlement_identity(
                    callback_authority_id,
                    callback_invoked_at,
                    accepted_semantic_bar_identity,
                    proof.strategy_id,
                    proof.account_id,
                    proof.full_instrument_id,
                    proof.accepted_bar_close_timestamp,
                    proof.batch_state_fingerprint,
                    proof.ordered_strategy_request_ids,
                    proof.intent_count_u8,
                    audit_commitment,
                    &success_seal,
                )
        };
        crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::
            construct_stage5e_validated_paper_settlement_receipt(
                self,
                audit_lineage,
                callback_invoked_at,
                callback_authority_id,
                settlement_identity,
                success_seal,
            )
    }

    #[cfg(test)]
    pub(crate) fn test_identity_proof_shape(
        &self,
    ) -> (Vec<StrategyRequestId>, usize, usize, bool, String) {
        let proof = self.borrow_identity_proof(&Stage5cB3fSuccessProofSeal(()));
        (
            proof.ordered_strategy_request_ids.to_vec(),
            usize::from(proof.intent_count_u8),
            proof.settled_batch_history_length,
            proof.canonical_first_batch_summary == &stage5ch_batch_summary(&self.settled.batch),
            proof.batch_state_fingerprint.to_string(),
        )
    }
}

impl Stage5eStage5cSettlementTerminalMaterial {
    pub(crate) fn construct_stage5e_terminal_receipt(
        self,
        audit_lineage: crate::stage5e_no_io_lifecycle::callback_authority::Stage5eAuthorizedCallbackAuditLineage,
        callback_invoked_at: DateTime<Utc>,
        callback_authority_id: [u8; 32],
        audit_commitment: [u8; 32],
        terminal_seal: crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::Stage5ePaperSettlementTerminalSeal,
    ) -> crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::Stage5ePaperSettlementTerminalReceipt{
        let Self {
            mutated_strategy,
            recovery_receipt,
            pre_callback_attribution_snapshot,
            retained_bar_metadata,
            exact_stage5c_intent_settlement_error,
            derived_original_intent_count,
        } = self;
        let reason = crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::
            map_stage5c_settlement_error_exact(
                exact_stage5c_intent_settlement_error,
                &terminal_seal,
            );
        crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::
            construct_stage5e_paper_settlement_terminal_receipt(
                mutated_strategy,
                recovery_receipt,
                pre_callback_attribution_snapshot,
                retained_bar_metadata,
                audit_lineage,
                callback_invoked_at,
                callback_authority_id,
                reason,
                exact_stage5c_intent_settlement_error,
                derived_original_intent_count,
                audit_commitment,
                terminal_seal,
            )
    }
}

#[allow(clippy::result_large_err)]
pub(crate) fn settle_stage5e_callback_escrow_material(
    material: Stage5eStage5cSettlementMaterial,
    _seal: Stage5cB3fSettlementSeal,
) -> Result<Stage5eStage5cSettlementSuccess, Stage5eStage5cSettlementTerminalMaterial> {
    let Stage5eStage5cSettlementMaterial {
        mutated_strategy,
        recovery_receipt,
        pre_callback_attribution_snapshot,
        retained_bar_metadata,
        exact_intent_vector,
        derived_original_intent_count,
    } = material;
    let admission = &recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    let expected_attribution_by_request =
        match stage5cj_expected_generated_attribution_by_request_from_ledger(
            admission,
            retained_bar_metadata.accepted_bar_close_ts,
            &exact_intent_vector,
            &pre_callback_attribution_snapshot.cleanup_ledger,
        ) {
            Ok(expected) => expected,
            Err(exact_stage5c_intent_settlement_error) => {
                drop(exact_intent_vector);
                return Err(Stage5eStage5cSettlementTerminalMaterial {
                    mutated_strategy,
                    recovery_receipt,
                    pre_callback_attribution_snapshot,
                    retained_bar_metadata,
                    exact_stage5c_intent_settlement_error,
                    derived_original_intent_count,
                });
            }
        };
    match settle_stage5c_semantic_result_owning_core(
        mutated_strategy,
        recovery_receipt,
        retained_bar_metadata.accepted_bar_close_ts,
        retained_bar_metadata.accepted_bar_origin,
        retained_bar_metadata.execution_eligible,
        exact_intent_vector,
        expected_attribution_by_request,
    ) {
        Ok(settled) => Ok(Stage5eStage5cSettlementSuccess { settled }),
        Err(failure) => Err(Stage5eStage5cSettlementTerminalMaterial {
            mutated_strategy: failure.strategy,
            recovery_receipt: failure.recovery_receipt,
            pre_callback_attribution_snapshot,
            retained_bar_metadata,
            exact_stage5c_intent_settlement_error: failure.error,
            derived_original_intent_count,
        }),
    }
}
// STAGE5E-B3F-SETTLEMENT-IMPLEMENTATION-END: stage5c-private-bridge-v1
// STAGE5E-B3E-CALLBACK-IMPLEMENTATION-END: private-materialization-v1
// STAGE5D-ADDITIVE-BRIDGE-END: stage5e-b3e-callback-materialization

fn apply_stage5c_semantic_bar_at(
    recovered: Stage5cPendingRecoveredPaperStrategy,
    accepted: Stage5cAcceptedSemanticBar,
    now: DateTime<Utc>,
) -> Result<Stage5cSemanticBarResult, Stage5cSemanticBarError> {
    let (mut strategy, recovery_receipt) = recovered.into_parts();
    let admission = &recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    if now
        > recovery_receipt
            .warmup_receipt()
            .restore_receipt()
            .bootstrap_receipt()
            .expires_at()
    {
        return Err(Stage5cSemanticBarError::BrokerTruthExpired);
    }
    if accepted.bar.instrument != *admission.target_instrument() {
        return Err(Stage5cSemanticBarError::InstrumentMismatch);
    }
    if !same_tick_size(accepted.tick_size, admission.tick_size()) {
        return Err(Stage5cSemanticBarError::TickSizeMismatch);
    }
    if accepted.bar.close_time_utc <= recovery_receipt.recovered_ts().timestamp()
        || accepted.bar.close_time_utc <= recovery_receipt.warmup_receipt().last_history_ts()
    {
        return Err(Stage5cSemanticBarError::StaleOrDuplicateBar);
    }
    if accepted.bar.close_time_utc > now.timestamp() {
        return Err(Stage5cSemanticBarError::FutureBar);
    }
    let pre_callback_cleanup_ledger =
        stage5cj_cleanup_attribution_ledger(Strategy::state(&strategy), admission.strategy_id());
    let context = stage5cf_semantic_context(&strategy, admission, accepted.bar.close_time_utc, now);
    let bar_close_ts = accepted.bar.close_time_utc;
    let origin = accepted.origin;
    let execution_eligible = origin == broker_core::HybridRuntimeBarOrigin::Live;
    let intents = crate::BrokerNeutralHybridStrategy::on_broker_bar(
        &mut strategy,
        broker_core::HybridRuntimeCallbackInput {
            context,
            payload: accepted.bar,
        },
    )
    .map_err(|_| Stage5cSemanticBarError::CallbackValidationFailed)?;
    let expected_attribution_by_request =
        stage5cj_expected_generated_attribution_by_request_from_ledger(
            admission,
            bar_close_ts,
            &intents,
            &pre_callback_cleanup_ledger,
        )
        .map_err(|_| Stage5cSemanticBarError::CallbackValidationFailed)?;
    Ok(Stage5cSemanticBarResult {
        strategy,
        recovery_receipt,
        bar_close_ts,
        origin,
        execution_eligible,
        intents,
        expected_attribution_by_request,
    })
}

fn stage5cf_semantic_context(
    strategy: &HybridIntradayRuntimeStrategy,
    admission: &Stage5cPaperHostAdmission,
    bar_close_ts: i64,
    now: DateTime<Utc>,
) -> broker_core::HybridRuntimeStrategyContext {
    broker_core::HybridRuntimeStrategyContext {
        strategy_id: admission.strategy_id().to_string(),
        request_namespace_account: admission.account_id().clone(),
        instrument: admission.target_instrument().clone(),
        tick_size: admission.tick_size(),
        trade_mode: broker_core::HybridRuntimeTradeMode::Paper,
        paper_execution_mode: broker_core::HybridRuntimePaperExecutionMode::LiveOnly,
        allow_live_orders: false,
        gateway_phase: broker_core::HybridRuntimeGatewayPhase::LiveReady,
        position_qty: Some(strategy.stage5c_current_position_qty()),
        event_ts_utc: bar_close_ts,
        strategy_now_ts_utc: now.timestamp(),
        last_bar_ts_utc: Some(bar_close_ts),
    }
}

pub fn settle_stage5c_semantic_result(
    result: Stage5cSemanticBarResult,
) -> Result<Stage5cSettledPaperStrategy, Stage5cIntentSettlementError> {
    settle_stage5c_semantic_result_with_expected_attribution(result, HashMap::new())
}

fn settle_stage5c_semantic_result_with_expected_attribution(
    result: Stage5cSemanticBarResult,
    expected_attribution_by_request: HashMap<
        StrategyRequestId,
        broker_core::HybridRuntimeAttribution,
    >,
) -> Result<Stage5cSettledPaperStrategy, Stage5cIntentSettlementError> {
    let (
        strategy,
        recovery_receipt,
        bar_close_ts,
        origin,
        execution_eligible,
        intents,
        mut result_expected_attribution_by_request,
    ) = result.into_parts();
    result_expected_attribution_by_request.extend(expected_attribution_by_request);
    settle_stage5c_semantic_result_owning_core(
        strategy,
        recovery_receipt,
        bar_close_ts,
        origin,
        execution_eligible,
        intents,
        result_expected_attribution_by_request,
    )
    .map_err(|failure| failure.error)
}

struct Stage5cOwningSettlementFailure {
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    error: Stage5cIntentSettlementError,
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::result_large_err)]
fn settle_stage5c_semantic_result_owning_core(
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    bar_close_ts: i64,
    origin: broker_core::HybridRuntimeBarOrigin,
    execution_eligible: bool,
    intents: Vec<crate::BrokerNeutralHybridIntent>,
    expected_attribution_by_request: HashMap<
        StrategyRequestId,
        broker_core::HybridRuntimeAttribution,
    >,
) -> Result<Stage5cSettledPaperStrategy, Stage5cOwningSettlementFailure> {
    let admission = &recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    if !execution_eligible && !intents.is_empty() {
        drop(intents);
        return Err(Stage5cOwningSettlementFailure {
            strategy,
            recovery_receipt,
            error: Stage5cIntentSettlementError::ReplayIntentNotExecutable,
        });
    }
    let batch = match stage5c_build_paper_intent_batch(
        &strategy,
        admission,
        bar_close_ts,
        origin,
        intents,
        &expected_attribution_by_request,
    ) {
        Ok(batch) => batch,
        Err(error) => {
            return Err(Stage5cOwningSettlementFailure {
                strategy,
                recovery_receipt,
                error,
            });
        }
    };
    let settled_batch_history = vec![stage5ch_batch_summary(&batch)];
    Ok(Stage5cSettledPaperStrategy {
        strategy,
        recovery_receipt,
        batch,
        settled_batch_history,
    })
}

#[cfg(test)]
mod stage5e_b3f_stage5c_settlement_tests {
    use super::*;

    // STAGE5G-C-R2CA-R2-TEST-CLOCK-BEGIN: deterministic-terminal-fill-test-clock-v1
    // These inherited parity tests used process date through `Utc::now()` and
    // became non-reproducible on weekends. Shadow only their local clock with
    // a fixed weekday; production code and every other test module keep the
    // real chrono type.
    struct Utc;

    impl Utc {
        fn now() -> chrono::DateTime<chrono::Utc> {
            chrono::DateTime::<chrono::Utc>::from_timestamp(1_767_679_800, 0)
                .expect("fixed B3F parity test timestamp")
        }
    }
    // STAGE5G-C-R2CA-R2-TEST-CLOCK-END: deterministic-terminal-fill-test-clock-v1

    fn set_pending_entry_for_b3f_test(
        strategy: &mut HybridIntradayRuntimeStrategy,
        request_id: StrategyRequestId,
    ) {
        let mut state = Strategy::state(strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                pending_entry_request_id,
                ..
            } => *pending_entry_request_id = Some(request_id),
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(strategy, state);
    }

    fn valid_entry_market_for_b3f_test() -> crate::BrokerNeutralHybridIntent {
        crate::BrokerNeutralHybridIntent::Market {
            qty: 1.0,
            side: crate::BrokerNeutralOrderSide::Buy,
            fill_price: Some(2227.5),
            comment: None,
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::Entry)
        .with_symbol("IMOEXF")
    }

    #[test]
    fn b3f_owning_core_matches_legacy_public_zero_intent_settlement() {
        let now = Utc::now();
        let predecessor = now.timestamp() - 600;
        let (legacy_recovered, _) =
            stage5e_test_nonempty_intent_sequence_inputs(now, predecessor, now.timestamp());
        let (core_recovered, _) =
            stage5e_test_nonempty_intent_sequence_inputs(now, predecessor, now.timestamp());
        let (legacy_strategy, legacy_receipt) = legacy_recovered.into_parts();
        let (core_strategy, core_receipt) = core_recovered.into_parts();

        let legacy = settle_stage5c_semantic_result(Stage5cSemanticBarResult {
            strategy: legacy_strategy,
            recovery_receipt: legacy_receipt,
            bar_close_ts: now.timestamp(),
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            execution_eligible: true,
            intents: Vec::new(),
            expected_attribution_by_request: HashMap::new(),
        })
        .expect("legacy zero-intent settlement must pass");
        let core = settle_stage5c_semantic_result_owning_core(
            core_strategy,
            core_receipt,
            now.timestamp(),
            broker_core::HybridRuntimeBarOrigin::Live,
            true,
            Vec::new(),
            HashMap::new(),
        )
        .unwrap_or_else(|_| panic!("B3F owning core zero-intent settlement must pass"));

        assert_eq!(
            stage5ch_batch_summary(legacy.intent_batch()),
            stage5ch_batch_summary(core.intent_batch())
        );
        assert_eq!(legacy.settled_batch_history(), core.settled_batch_history());
        assert_eq!(legacy.settled_batch_history().len(), 1);
    }

    #[test]
    fn b3f_owning_core_matches_legacy_public_nonempty_settlement() {
        let now = Utc::now();
        let bar_close_ts = now.timestamp() + 600;
        let (legacy_recovered, _) =
            stage5e_test_nonempty_intent_sequence_inputs(now, now.timestamp() - 600, bar_close_ts);
        let (core_recovered, _) =
            stage5e_test_nonempty_intent_sequence_inputs(now, now.timestamp() - 600, bar_close_ts);
        let (mut legacy_strategy, legacy_receipt) = legacy_recovered.into_parts();
        let (mut core_strategy, core_receipt) = core_recovered.into_parts();
        let request_id = crate::deterministic_request_id(
            "stage5e_test",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            3,
        );
        set_pending_entry_for_b3f_test(&mut legacy_strategy, request_id);
        set_pending_entry_for_b3f_test(&mut core_strategy, request_id);
        let intent = valid_entry_market_for_b3f_test();
        let legacy = settle_stage5c_semantic_result(Stage5cSemanticBarResult {
            strategy: legacy_strategy,
            recovery_receipt: legacy_receipt,
            bar_close_ts,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            execution_eligible: true,
            intents: vec![intent.clone()],
            expected_attribution_by_request: HashMap::new(),
        })
        .expect("legacy non-empty settlement must pass");
        let core = settle_stage5c_semantic_result_owning_core(
            core_strategy,
            core_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            true,
            vec![intent],
            HashMap::new(),
        )
        .unwrap_or_else(|_| panic!("B3F owning core non-empty settlement must pass"));

        assert_eq!(
            stage5ch_batch_summary(legacy.intent_batch()),
            stage5ch_batch_summary(core.intent_batch())
        );
        assert_eq!(legacy.settled_batch_history(), core.settled_batch_history());
    }

    #[test]
    fn b3f_owning_core_matches_legacy_public_representative_error() {
        let now = Utc::now();
        let bar_close_ts = now.timestamp() + 600;
        let (legacy_recovered, _) =
            stage5e_test_nonempty_intent_sequence_inputs(now, now.timestamp() - 600, bar_close_ts);
        let (core_recovered, _) =
            stage5e_test_nonempty_intent_sequence_inputs(now, now.timestamp() - 600, bar_close_ts);
        let (mut legacy_strategy, legacy_receipt) = legacy_recovered.into_parts();
        let (mut core_strategy, core_receipt) = core_recovered.into_parts();
        let invalid_intent = crate::BrokerNeutralHybridIntent::Market {
            qty: -1.0,
            side: crate::BrokerNeutralOrderSide::Buy,
            fill_price: None,
            comment: None,
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::Entry)
        .with_symbol("IMOEXF");
        let request_id = crate::deterministic_request_id(
            "stage5e_test",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            3,
        );
        set_pending_entry_for_b3f_test(&mut legacy_strategy, request_id);
        set_pending_entry_for_b3f_test(&mut core_strategy, request_id);
        let legacy_error = settle_stage5c_semantic_result(Stage5cSemanticBarResult {
            strategy: legacy_strategy,
            recovery_receipt: legacy_receipt,
            bar_close_ts,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            execution_eligible: true,
            intents: vec![invalid_intent.clone()],
            expected_attribution_by_request: HashMap::new(),
        })
        .expect_err("invalid quantity must fail legacy settlement");
        let core_error = settle_stage5c_semantic_result_owning_core(
            core_strategy,
            core_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            true,
            vec![invalid_intent],
            HashMap::new(),
        )
        .expect_err("invalid quantity must fail owning core")
        .error;
        assert_eq!(legacy_error, core_error);
    }
}

fn stage5c_build_paper_intent_batch(
    strategy: &HybridIntradayRuntimeStrategy,
    admission: &Stage5cPaperHostAdmission,
    bar_close_ts: i64,
    origin: broker_core::HybridRuntimeBarOrigin,
    intents: Vec<crate::BrokerNeutralHybridIntent>,
    expected_attribution_by_request: &HashMap<
        StrategyRequestId,
        broker_core::HybridRuntimeAttribution,
    >,
) -> Result<Stage5cPaperIntentBatch, Stage5cIntentSettlementError> {
    if intents.len() > u8::MAX as usize {
        return Err(Stage5cIntentSettlementError::TooManyIntents);
    }
    let mut request_ids = Vec::with_capacity(intents.len());
    let mut records = Vec::with_capacity(intents.len());
    let mut seen_request_ids = HashSet::new();
    let state = Strategy::state(strategy);
    for intent in intents {
        validate_stage5cg_intent(
            &intent,
            &admission.target_instrument().symbol,
            admission.tick_size(),
            bar_close_ts,
        )?;
        let class = intent
            .explicit_class()
            .ok_or(Stage5cIntentSettlementError::MissingIntentClass)?;
        let request_id = stage5cg_source_request_id(
            admission.strategy_id(),
            admission.account_id().as_str(),
            &admission.target_instrument().symbol,
            bar_close_ts,
            &intent,
        )?;
        stage5cg_verify_pending_request_id(state, class, request_id)?;
        if !seen_request_ids.insert(request_id) {
            return Err(Stage5cIntentSettlementError::DuplicateRequestId);
        }
        request_ids.push(request_id);
        let expected_attribution = expected_attribution_by_request
            .get(&request_id)
            .cloned()
            .or_else(|| {
                stage5cj_expected_attribution_for_intent(
                    state,
                    admission.strategy_id(),
                    class,
                    &intent,
                )
            });
        records.push(Stage5cPaperIntentRecord {
            request_id,
            source_event_ts: bar_close_ts,
            intent_class: class,
            expected_attribution,
            intent,
        });
    }
    Ok(Stage5cPaperIntentBatch {
        strategy_id: admission.strategy_id().to_string(),
        account_id: admission.account_id().clone(),
        instrument: admission.target_instrument().clone(),
        bar_close_ts,
        state_fingerprint: stage5c_state_fingerprint(state),
        request_ids,
        records,
        observation_only: origin == broker_core::HybridRuntimeBarOrigin::Replay,
    })
}

fn stage5ch_batch_summary(batch: &Stage5cPaperIntentBatch) -> Stage5cPaperIntentBatchSummary {
    let min_source_event_ts = batch
        .records
        .iter()
        .map(|record| record.source_event_ts)
        .min()
        .unwrap_or(batch.bar_close_ts);
    let max_source_event_ts = batch
        .records
        .iter()
        .map(|record| record.source_event_ts)
        .max()
        .unwrap_or(batch.bar_close_ts);
    Stage5cPaperIntentBatchSummary {
        strategy_id: batch.strategy_id.clone(),
        account_id: batch.account_id.clone(),
        instrument: batch.instrument.clone(),
        origin_bar_close_ts: batch.bar_close_ts,
        bar_close_ts: batch.bar_close_ts,
        min_source_event_ts,
        max_source_event_ts,
        state_fingerprint: batch.state_fingerprint.clone(),
        request_ids: batch.request_ids.clone(),
        intent_count: batch.intent_count(),
        observation_only: batch.observation_only,
    }
}

fn stage5c_state_fingerprint(state: &StrategyState) -> String {
    let state_bytes = serde_json::to_vec(state).expect("strategy state is serializable");
    format!("{:x}", Sha256::digest(&state_bytes))
}

pub fn advance_stage5c_controlled_next_bar(
    settled: Stage5cSettledPaperStrategy,
    accepted: Stage5cAcceptedSemanticBar,
) -> Result<Stage5cSettledPaperStrategy, Stage5cNextBarLoopFailure> {
    advance_stage5c_controlled_next_bar_at(settled, accepted, Utc::now())
}

fn advance_stage5c_controlled_next_bar_at(
    settled: Stage5cSettledPaperStrategy,
    accepted: Stage5cAcceptedSemanticBar,
    now: DateTime<Utc>,
) -> Result<Stage5cSettledPaperStrategy, Stage5cNextBarLoopFailure> {
    if accepted.bar.close_time_utc <= settled.batch.bar_close_ts() {
        return Err(Stage5cNextBarLoopFailure::Blocked(Box::new(
            Stage5cNextBarBlocked {
                reason: Stage5cNextBarLoopError::NonMonotonicBar,
                settled,
            },
        )));
    }
    if settled.batch.intent_count() > 0 {
        return Err(Stage5cNextBarLoopFailure::Blocked(Box::new(
            Stage5cNextBarBlocked {
                reason: Stage5cNextBarLoopError::UnresolvedIntentBatch,
                settled,
            },
        )));
    }
    if now
        > settled
            .recovery_receipt
            .warmup_receipt()
            .restore_receipt()
            .bootstrap_receipt()
            .expires_at()
    {
        return Err(Stage5cNextBarLoopFailure::Blocked(Box::new(
            Stage5cNextBarBlocked {
                reason: Stage5cNextBarLoopError::Semantic(
                    Stage5cSemanticBarError::BrokerTruthExpired,
                ),
                settled,
            },
        )));
    }
    let mut history = settled.settled_batch_history.clone();
    let (strategy, recovery_receipt, _) = settled.into_parts();
    let recovered = Stage5cPendingRecoveredPaperStrategy {
        strategy,
        receipt: recovery_receipt,
    };
    let semantic = apply_stage5c_semantic_bar_at(recovered, accepted, now).map_err(|reason| {
        Stage5cNextBarLoopFailure::Failed(Stage5cNextBarLoopError::Semantic(reason))
    })?;
    let mut next = settle_stage5c_semantic_result(semantic).map_err(|reason| {
        Stage5cNextBarLoopFailure::Failed(Stage5cNextBarLoopError::Settlement(reason))
    })?;
    history.push(stage5ch_batch_summary(next.intent_batch()));
    next.settled_batch_history = history;
    Ok(next)
}

pub fn resolve_stage5c_paper_intent_lifecycle(
    settled: Stage5cSettledPaperStrategy,
    input: Stage5cPaperIntentLifecycleInput,
) -> Result<Stage5cResolvedPaperIntentBatchStrategy, Stage5cPaperIntentLifecycleFailure> {
    if settled.batch.intent_count() == 0 {
        return Err(Stage5cPaperIntentLifecycleFailure::Blocked(Box::new(
            Stage5cPaperIntentLifecycleBlocked {
                reason: Stage5cPaperIntentLifecycleError::EmptyIntentBatch,
                settled,
            },
        )));
    }
    let state_fingerprint = stage5c_state_fingerprint(Strategy::state(&settled.strategy));
    if state_fingerprint != settled.batch.state_fingerprint {
        return Err(Stage5cPaperIntentLifecycleFailure::Blocked(Box::new(
            Stage5cPaperIntentLifecycleBlocked {
                reason: Stage5cPaperIntentLifecycleError::StateFingerprintMismatch,
                settled,
            },
        )));
    }
    let expected_request_ids: HashSet<StrategyRequestId> =
        settled.batch.request_ids.iter().copied().collect();
    let source_ts_by_request: HashMap<StrategyRequestId, i64> = settled
        .batch
        .records
        .iter()
        .map(|record| (record.request_id, record.source_event_ts))
        .collect();
    let mut seen = HashSet::new();
    let mut sequences = HashSet::new();
    for record in &input.ack_records {
        if !sequences.insert(record.total_sequence) {
            return Err(Stage5cPaperIntentLifecycleFailure::Blocked(Box::new(
                Stage5cPaperIntentLifecycleBlocked {
                    reason: Stage5cPaperIntentLifecycleError::DuplicateSequence,
                    settled,
                },
            )));
        }
        let Some(source_event_ts) = source_ts_by_request.get(&record.ack.request_id) else {
            return Err(Stage5cPaperIntentLifecycleFailure::Blocked(Box::new(
                Stage5cPaperIntentLifecycleBlocked {
                    reason: Stage5cPaperIntentLifecycleError::UnknownAckRequestId,
                    settled,
                },
            )));
        };
        if record.ack.processed_ts_utc < *source_event_ts {
            return Err(Stage5cPaperIntentLifecycleFailure::Blocked(Box::new(
                Stage5cPaperIntentLifecycleBlocked {
                    reason: Stage5cPaperIntentLifecycleError::AckTimestampBeforeIntentBar,
                    settled,
                },
            )));
        }
        if !expected_request_ids.contains(&record.ack.request_id) {
            return Err(Stage5cPaperIntentLifecycleFailure::Blocked(Box::new(
                Stage5cPaperIntentLifecycleBlocked {
                    reason: Stage5cPaperIntentLifecycleError::UnknownAckRequestId,
                    settled,
                },
            )));
        }
        if !seen.insert(record.ack.request_id) {
            return Err(Stage5cPaperIntentLifecycleFailure::Blocked(Box::new(
                Stage5cPaperIntentLifecycleBlocked {
                    reason: Stage5cPaperIntentLifecycleError::DuplicateAck,
                    settled,
                },
            )));
        }
    }
    if seen.len() != expected_request_ids.len() {
        return Err(Stage5cPaperIntentLifecycleFailure::Blocked(Box::new(
            Stage5cPaperIntentLifecycleBlocked {
                reason: Stage5cPaperIntentLifecycleError::MissingAck,
                settled,
            },
        )));
    }
    let Stage5cSettledPaperStrategy {
        mut strategy,
        recovery_receipt,
        batch,
        settled_batch_history,
    } = settled;
    let mut ack_records = input.ack_records;
    ack_records.sort_by_key(|record| record.total_sequence);
    let mut last_sequence = None;
    let mut ack_outcomes = Vec::with_capacity(ack_records.len());
    for record in ack_records {
        if last_sequence.is_some_and(|previous| record.total_sequence <= previous) {
            return Err(Stage5cPaperIntentLifecycleFailure::Terminal(
                Stage5cPaperIntentLifecycleError::NonMonotonicSequence,
            ));
        }
        last_sequence = Some(record.total_sequence);
        let outcome = Stage5cPaperAckOutcome {
            total_sequence: record.total_sequence,
            request_id: record.ack.request_id,
            status: record.ack.status,
            broker_order_id: record.ack.broker_order_id.clone(),
            error_code: record.ack.error_code.clone(),
            processed_ts_utc: record.ack.processed_ts_utc,
        };
        let intents = crate::BrokerNeutralHybridStrategy::on_broker_ack(&mut strategy, record.ack)
            .map_err(|_| {
                Stage5cPaperIntentLifecycleFailure::Terminal(
                    Stage5cPaperIntentLifecycleError::CallbackValidationFailed,
                )
            })?;
        if !intents.is_empty() {
            return Err(Stage5cPaperIntentLifecycleFailure::Terminal(
                Stage5cPaperIntentLifecycleError::CallbackGeneratedIntentTerminal,
            ));
        }
        ack_outcomes.push(outcome);
    }
    Ok(Stage5cResolvedPaperIntentBatchStrategy {
        strategy,
        recovery_receipt,
        resolved_batch: batch,
        ack_outcomes,
        settled_batch_history,
    })
}

pub fn resolve_stage5c_paper_broker_lifecycle(
    resolved: Stage5cResolvedPaperIntentBatchStrategy,
    input: Stage5cPaperBrokerLifecycleInput,
) -> Result<Stage5cBrokerLifecycleResolvedPaperStrategy, Stage5cPaperBrokerLifecycleFailure> {
    let mut sequences = HashSet::new();
    let mut event_identity_records: HashMap<String, Stage5cPaperBrokerEventRecord> = HashMap::new();
    for record in &input.event_records {
        if !sequences.insert(record.total_sequence) {
            return Err(stage5cj_block(
                Stage5cPaperBrokerLifecycleError::DuplicateSequence,
                resolved,
            ));
        }
        if !resolved
            .resolved_batch
            .request_ids
            .contains(&record.request_id)
        {
            return Err(stage5cj_block(
                Stage5cPaperBrokerLifecycleError::UnknownEventRequestId,
                resolved,
            ));
        }
        if record.payload.instrument() != resolved.resolved_batch.instrument() {
            return Err(stage5cj_block(
                Stage5cPaperBrokerLifecycleError::InstrumentMismatch,
                resolved,
            ));
        }
        let identity = match stage5cj_event_identity(record) {
            Ok(identity) => identity,
            Err(_) => {
                return Err(stage5cj_block(
                    Stage5cPaperBrokerLifecycleError::CallbackValidationFailed,
                    resolved,
                ));
            }
        };
        if let Some(previous) = event_identity_records.get_mut(&identity) {
            if record.payload != previous.payload {
                return Err(stage5cj_block(
                    Stage5cPaperBrokerLifecycleError::ConflictingDuplicateEvent,
                    resolved,
                ));
            }
            if record.total_sequence < previous.total_sequence {
                *previous = record.clone();
            }
            continue;
        }
        event_identity_records.insert(identity, record.clone());
    }
    let mut canonical_event_records: Vec<_> = event_identity_records.into_values().collect();
    canonical_event_records.sort_by_key(|record| record.total_sequence);
    let admission_strategy_id = resolved
        .recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission
        .strategy_id()
        .to_string();
    let ack_by_request: HashMap<StrategyRequestId, Stage5cPaperAckOutcome> = resolved
        .ack_outcomes
        .iter()
        .cloned()
        .map(|outcome| (outcome.request_id, outcome))
        .collect();
    let mut events_by_request: HashMap<StrategyRequestId, Vec<Stage5cPaperBrokerEventRecord>> =
        HashMap::new();
    for record in &canonical_event_records {
        events_by_request
            .entry(record.request_id)
            .or_default()
            .push(record.clone());
    }
    let mut remaining_lifecycle_expectations = Vec::new();
    for intent_record in &resolved.resolved_batch.records {
        let ack = ack_by_request
            .get(&intent_record.request_id)
            .expect("ACK lifecycle enforces exact request coverage");
        let request_events = events_by_request
            .get(&intent_record.request_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if stage5cj_ack_is_terminal(ack.status) {
            if !request_events.is_empty() {
                return Err(stage5cj_block(
                    Stage5cPaperBrokerLifecycleError::EventForTerminalAck,
                    resolved,
                ));
            }
            continue;
        }
        if request_events.is_empty() {
            return Err(stage5cj_block(
                Stage5cPaperBrokerLifecycleError::MissingExpectedBrokerEvent,
                resolved,
            ));
        }
        let mut terminal_seen = false;
        let mut simulated_position_qty = stage5cj_position_qty(Strategy::state(&resolved.strategy));
        for record in request_events {
            if record.payload.source_ts_utc() < ack.processed_ts_utc {
                return Err(stage5cj_block(
                    Stage5cPaperBrokerLifecycleError::EventTimestampBeforeAck,
                    resolved,
                ));
            }
            if !stage5cj_allowed_event_kinds(intent_record).contains(&record.payload.kind()) {
                return Err(stage5cj_block(
                    Stage5cPaperBrokerLifecycleError::UnexpectedBrokerEventKind,
                    resolved,
                ));
            }
            let validation = stage5cj_validate_event_mapping(
                record,
                ack,
                intent_record,
                &admission_strategy_id,
                simulated_position_qty,
            );
            if let Err(failure) = validation {
                return Err(match failure {
                    Stage5cPaperBrokerLifecycleFailure::Blocked(blocked) => {
                        Stage5cPaperBrokerLifecycleFailure::Blocked(blocked)
                    }
                    Stage5cPaperBrokerLifecycleFailure::Terminal(reason) => {
                        stage5cj_block(reason, resolved)
                    }
                });
            }
            if stage5cj_event_is_terminal_for_intent(record, intent_record, request_events) {
                terminal_seen = true;
            }
            if let Stage5cPaperBrokerEventPayload::Position(position) = &record.payload {
                simulated_position_qty = position.qty;
            }
        }
        if !terminal_seen {
            remaining_lifecycle_expectations.push(Stage5cPaperBrokerLifecycleExpectation {
                request_id: intent_record.request_id,
                expected_event_kind: stage5cj_next_expected_event_kind(
                    intent_record,
                    request_events,
                ),
                reason: "terminal_lifecycle_not_observed".to_string(),
            });
        }
    }
    let Stage5cResolvedPaperIntentBatchStrategy {
        mut strategy,
        recovery_receipt,
        resolved_batch: batch,
        ack_outcomes,
        mut settled_batch_history,
    } = resolved;
    let admission = &recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    let broker_event_count = canonical_event_records.len();
    let lifecycle_watermark_ts_utc =
        stage5ck_lifecycle_watermark_ts_utc(&batch, &ack_outcomes, &canonical_event_records);
    let mut generated_intent_batch: Option<Stage5cPaperIntentBatch> = None;
    for record in canonical_event_records {
        let Some(_intent_record) = batch
            .records
            .iter()
            .find(|intent| intent.request_id == record.request_id)
        else {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::UnknownEventRequestId,
            ));
        };
        let Some(_ack) = ack_by_request.get(&record.request_id) else {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::UnknownEventRequestId,
            ));
        };
        let source_ts = record.payload.source_ts_utc();
        let context = stage5cj_broker_lifecycle_context(
            &strategy,
            admission,
            batch.bar_close_ts(),
            source_ts,
        );
        let cleanup_ledger = stage5cj_cleanup_attribution_ledger(
            Strategy::state(&strategy),
            admission.strategy_id(),
        );
        let intents = match record.payload {
            Stage5cPaperBrokerEventPayload::Order(payload) => {
                crate::BrokerNeutralHybridStrategy::on_broker_order(
                    &mut strategy,
                    broker_core::HybridRuntimeCallbackInput { context, payload },
                )
            }
            Stage5cPaperBrokerEventPayload::StopOrder(payload) => {
                crate::BrokerNeutralHybridStrategy::on_broker_stop_order(
                    &mut strategy,
                    broker_core::HybridRuntimeCallbackInput { context, payload },
                )
            }
            Stage5cPaperBrokerEventPayload::Position(payload) => {
                crate::BrokerNeutralHybridStrategy::on_broker_position(
                    &mut strategy,
                    broker_core::HybridRuntimeCallbackInput { context, payload },
                )
            }
        }
        .map_err(|_| {
            Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::CallbackValidationFailed,
            )
        })?;
        if !intents.is_empty() {
            let expected_attribution_by_request =
                stage5cj_expected_generated_attribution_by_request_from_ledger(
                    admission,
                    source_ts,
                    &intents,
                    &cleanup_ledger,
                )
                .map_err(|_| {
                    Stage5cPaperBrokerLifecycleFailure::Terminal(
                        Stage5cPaperBrokerLifecycleError::CallbackGeneratedIntentTerminal,
                    )
                })?;
            let callback_batch = stage5c_build_paper_intent_batch(
                &strategy,
                admission,
                source_ts,
                broker_core::HybridRuntimeBarOrigin::Live,
                intents,
                &expected_attribution_by_request,
            )
            .map_err(|_| {
                Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::CallbackGeneratedIntentTerminal,
                )
            })?;
            stage5cj_merge_generated_batch(&mut generated_intent_batch, callback_batch).map_err(
                |_| {
                    Stage5cPaperBrokerLifecycleFailure::Terminal(
                        Stage5cPaperBrokerLifecycleError::CallbackGeneratedIntentTerminal,
                    )
                },
            )?;
        }
    }
    if let Some(generated_batch) = &mut generated_intent_batch {
        stage5cj_verify_generated_batch_final_pending_consistency(
            Strategy::state(&strategy),
            generated_batch,
        )
        .map_err(|_| {
            Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::CallbackGeneratedIntentTerminal,
            )
        })?;
        generated_batch.state_fingerprint = stage5c_state_fingerprint(Strategy::state(&strategy));
        settled_batch_history.push(stage5ch_batch_summary(generated_batch));
    }
    let resolved_batch_summary = stage5ch_batch_summary(&batch);
    Ok(Stage5cBrokerLifecycleResolvedPaperStrategy {
        strategy,
        recovery_receipt,
        resolved_batch: batch,
        resolved_batch_summary,
        ack_outcomes,
        broker_event_count,
        remaining_lifecycle_expectations,
        lifecycle_watermark_ts_utc,
        generated_intent_batch,
        settled_batch_history,
    })
}

pub fn resolve_stage5c_paper_timer(
    resolved: Stage5cBrokerLifecycleResolvedPaperStrategy,
    input: Stage5cPaperTimerInput,
) -> Result<Stage5cTimerResolvedPaperStrategy, Stage5cPaperTimerFailure> {
    if !resolved.remaining_lifecycle_expectations.is_empty() {
        return Err(stage5ck_block(
            Stage5cPaperTimerError::UnresolvedBrokerLifecycle,
            resolved,
        ));
    }
    if resolved.generated_intent_batch.is_some() {
        return Err(stage5ck_block(
            Stage5cPaperTimerError::UnresolvedGeneratedIntentBatch,
            resolved,
        ));
    }
    let lifecycle_watermark_ts_utc_ms = resolved.lifecycle_watermark_ts_utc.saturating_mul(1_000);
    if input.now_ts_utc_ms < lifecycle_watermark_ts_utc_ms {
        return Err(stage5ck_block(
            Stage5cPaperTimerError::NonMonotonicTimer,
            resolved,
        ));
    }
    let Some(timer_now) = Utc.timestamp_millis_opt(input.now_ts_utc_ms).single() else {
        return Err(stage5ck_block(
            Stage5cPaperTimerError::BrokerTruthExpired,
            resolved,
        ));
    };
    if timer_now
        > resolved
            .recovery_receipt
            .warmup_receipt()
            .restore_receipt()
            .bootstrap_receipt()
            .expires_at()
    {
        return Err(stage5ck_block(
            Stage5cPaperTimerError::BrokerTruthExpired,
            resolved,
        ));
    }
    let Stage5cBrokerLifecycleResolvedPaperStrategy {
        mut strategy,
        recovery_receipt,
        resolved_batch_summary,
        mut settled_batch_history,
        lifecycle_watermark_ts_utc,
        ..
    } = resolved;
    let admission = &recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    let timer_ts_utc = input.now_ts_utc_ms.div_euclid(1_000);
    let cleanup_ledger =
        stage5cj_cleanup_attribution_ledger(Strategy::state(&strategy), admission.strategy_id());
    let context = stage5ck_timer_context(&strategy, admission, lifecycle_watermark_ts_utc, input);
    let intents = crate::BrokerNeutralHybridStrategy::on_broker_timer(
        &mut strategy,
        broker_core::HybridRuntimeCallbackInput {
            context,
            payload: broker_core::HybridRuntimeTimerEvent {
                now_ts_utc_ms: input.now_ts_utc_ms,
            },
        },
    )
    .map_err(|_| {
        Stage5cPaperTimerFailure::Terminal(Stage5cPaperTimerError::CallbackValidationFailed)
    })?;
    let generated_intent_batch = if intents.is_empty() {
        None
    } else {
        let expected_attribution_by_request =
            stage5cj_expected_generated_attribution_by_request_from_ledger(
                admission,
                timer_ts_utc,
                &intents,
                &cleanup_ledger,
            )
            .map_err(|_| {
                Stage5cPaperTimerFailure::Terminal(Stage5cPaperTimerError::GeneratedIntentTerminal)
            })?;
        let batch = stage5c_build_paper_intent_batch(
            &strategy,
            admission,
            timer_ts_utc,
            broker_core::HybridRuntimeBarOrigin::Live,
            intents,
            &expected_attribution_by_request,
        )
        .map_err(|_| {
            Stage5cPaperTimerFailure::Terminal(Stage5cPaperTimerError::GeneratedIntentTerminal)
        })?;
        stage5cj_verify_generated_batch_final_pending_consistency(
            Strategy::state(&strategy),
            &batch,
        )
        .map_err(|_| {
            Stage5cPaperTimerFailure::Terminal(Stage5cPaperTimerError::GeneratedIntentTerminal)
        })?;
        settled_batch_history.push(stage5ch_batch_summary(&batch));
        Some(batch)
    };
    Ok(Stage5cTimerResolvedPaperStrategy {
        strategy,
        recovery_receipt,
        resolved_batch_summary,
        timer_ts_utc_ms: input.now_ts_utc_ms,
        generated_intent_batch,
        settled_batch_history,
    })
}

pub fn settle_stage5c_timer_result(
    timer: Stage5cTimerResolvedPaperStrategy,
) -> Stage5cTimerSettlement {
    let Stage5cTimerResolvedPaperStrategy {
        strategy,
        recovery_receipt,
        resolved_batch_summary,
        timer_ts_utc_ms,
        generated_intent_batch,
        mut settled_batch_history,
    } = timer;
    match generated_intent_batch {
        Some(batch) => {
            Stage5cTimerSettlement::generated_intent_batch(Stage5cSettledPaperStrategy {
                strategy,
                recovery_receipt,
                batch,
                settled_batch_history,
            })
        }
        None => {
            let batch = stage5cl_zero_timer_batch(
                &strategy,
                &recovery_receipt,
                &resolved_batch_summary,
                timer_ts_utc_ms,
            );
            settled_batch_history.push(stage5ch_batch_summary(&batch));
            Stage5cTimerSettlement::ready_for_continuation(
                Stage5cSettledPaperStrategy {
                    strategy,
                    recovery_receipt,
                    batch,
                    settled_batch_history,
                },
                timer_ts_utc_ms,
            )
        }
    }
}

pub fn advance_stage5c_timer_settlement_next_bar(
    settlement: Stage5cTimerSettlement,
    accepted: Stage5cAcceptedSemanticBar,
) -> Result<Stage5cSettledPaperStrategy, Stage5cTimerContinuationFailure> {
    advance_stage5c_timer_settlement_next_bar_at(settlement, accepted, Utc::now())
}

fn advance_stage5c_timer_settlement_next_bar_at(
    settlement: Stage5cTimerSettlement,
    accepted: Stage5cAcceptedSemanticBar,
    now: DateTime<Utc>,
) -> Result<Stage5cSettledPaperStrategy, Stage5cTimerContinuationFailure> {
    let (settled, checkpoint_ts_utc_ms) = match settlement.inner {
        Stage5cTimerSettlementKind::ReadyForContinuation {
            settled,
            checkpoint_ts_utc_ms,
        } => (settled, checkpoint_ts_utc_ms),
        Stage5cTimerSettlementKind::GeneratedIntentBatch(settled) => {
            return Err(stage5cm_block(
                Stage5cTimerContinuationError::GeneratedIntentBatchRequiresLifecycle,
                Stage5cTimerSettlement::generated_intent_batch(settled),
            ));
        }
    };
    match advance_stage5c_controlled_next_bar_at(settled, accepted, now) {
        Ok(advanced) => Ok(advanced),
        Err(Stage5cNextBarLoopFailure::Blocked(blocked)) => {
            let reason = blocked.reason();
            Err(stage5cm_block(
                Stage5cTimerContinuationError::NextBar(reason),
                Stage5cTimerSettlement::ready_for_continuation(
                    blocked.into_settled(),
                    checkpoint_ts_utc_ms,
                ),
            ))
        }
        Err(Stage5cNextBarLoopFailure::Failed(reason)) => {
            Err(Stage5cTimerContinuationFailure::Terminal(
                Stage5cTimerContinuationError::NextBar(reason),
            ))
        }
    }
}

// STAGE5G-D-R1A-AUTHORITY-BEGIN: deterministic-bar-continuation-authority-v1
#[allow(dead_code)] // Independently reviewed authority is consumed only by Stage 5G-d R1-b.
pub(crate) fn stage5gd_accepted_bar_checkpoint_ts_utc_ms(
    accepted: &Stage5cAcceptedSemanticBar,
) -> Result<i64, Stage5cTimerContinuationError> {
    accepted
        .bar
        .close_time_utc
        .checked_mul(1_000)
        .ok_or(Stage5cTimerContinuationError::NextBar(
            Stage5cNextBarLoopError::Semantic(Stage5cSemanticBarError::InvalidTimestamp),
        ))
}

#[allow(dead_code)] // Independently reviewed authority is consumed only by Stage 5G-d R1-b.
pub(crate) fn advance_stage5c_timer_settlement_next_bar_at_checkpoint(
    settlement: Stage5cTimerSettlement,
    accepted: Stage5cAcceptedSemanticBar,
    explicit_now_ts_utc_ms: i64,
    previous_continuation_checkpoint_ts_utc_ms: i64,
) -> Result<Stage5cSettledPaperStrategy, Stage5cTimerContinuationFailure> {
    let bar_checkpoint_ts_utc_ms = match stage5gd_accepted_bar_checkpoint_ts_utc_ms(&accepted) {
        Ok(checkpoint) => checkpoint,
        Err(reason) => return Err(stage5cm_block(reason, settlement)),
    };
    if bar_checkpoint_ts_utc_ms <= previous_continuation_checkpoint_ts_utc_ms {
        return Err(stage5cm_block(
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::NonMonotonicBar),
            settlement,
        ));
    }
    let Some(explicit_now) = Utc.timestamp_millis_opt(explicit_now_ts_utc_ms).single() else {
        return Err(stage5cm_block(
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::Semantic(
                Stage5cSemanticBarError::InvalidTimestamp,
            )),
            settlement,
        ));
    };
    advance_stage5c_timer_settlement_next_bar_at(settlement, accepted, explicit_now)
}
// STAGE5G-D-R1A-AUTHORITY-END: deterministic-bar-continuation-authority-v1

// STAGE5G-D-R1A-R1-AUTHORITY-BEGIN: complete-precallback-transactional-admission-v1
#[cfg(test)]
thread_local! {
    static STAGE5GD_R1A_R1_DELEGATE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn stage5gd_r1a_r1_reset_delegate_count() {
    STAGE5GD_R1A_R1_DELEGATE_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
fn stage5gd_r1a_r1_delegate_count() -> usize {
    STAGE5GD_R1A_R1_DELEGATE_COUNT.with(std::cell::Cell::get)
}

#[allow(dead_code)] // Independently reviewed authority is consumed only by Stage 5G-d R1-b.
pub(crate) fn advance_stage5c_timer_settlement_next_bar_transactional_at_checkpoint(
    settlement: Stage5cTimerSettlement,
    accepted: Stage5cAcceptedSemanticBar,
    explicit_now_ts_utc_ms: i64,
    previous_continuation_checkpoint_ts_utc_ms: i64,
) -> Result<Stage5cSettledPaperStrategy, Stage5cTimerContinuationFailure> {
    let settled = match &settlement.inner {
        Stage5cTimerSettlementKind::ReadyForContinuation { settled, .. } => settled,
        Stage5cTimerSettlementKind::GeneratedIntentBatch(_) => {
            return Err(stage5cm_block(
                Stage5cTimerContinuationError::GeneratedIntentBatchRequiresLifecycle,
                settlement,
            ));
        }
    };
    if settled.batch.intent_count() > 0 {
        return Err(stage5cm_block(
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::UnresolvedIntentBatch),
            settlement,
        ));
    }
    let bar_checkpoint_ts_utc_ms = match stage5gd_accepted_bar_checkpoint_ts_utc_ms(&accepted) {
        Ok(checkpoint) => checkpoint,
        Err(reason) => return Err(stage5cm_block(reason, settlement)),
    };
    if bar_checkpoint_ts_utc_ms <= previous_continuation_checkpoint_ts_utc_ms {
        return Err(stage5cm_block(
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::NonMonotonicBar),
            settlement,
        ));
    }
    let recovery_receipt = &settled.recovery_receipt;
    let admission = &recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    if accepted.bar.instrument != *admission.target_instrument() {
        return Err(stage5cm_block(
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::Semantic(
                Stage5cSemanticBarError::InstrumentMismatch,
            )),
            settlement,
        ));
    }
    if !same_tick_size(accepted.tick_size, admission.tick_size()) {
        return Err(stage5cm_block(
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::Semantic(
                Stage5cSemanticBarError::TickSizeMismatch,
            )),
            settlement,
        ));
    }
    if accepted.bar.close_time_utc <= recovery_receipt.recovered_ts().timestamp()
        || accepted.bar.close_time_utc <= recovery_receipt.warmup_receipt().last_history_ts()
    {
        return Err(stage5cm_block(
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::Semantic(
                Stage5cSemanticBarError::StaleOrDuplicateBar,
            )),
            settlement,
        ));
    }
    if accepted.bar.close_time_utc <= settled.batch.bar_close_ts() {
        return Err(stage5cm_block(
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::NonMonotonicBar),
            settlement,
        ));
    }
    let Some(explicit_now) = Utc.timestamp_millis_opt(explicit_now_ts_utc_ms).single() else {
        return Err(stage5cm_block(
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::Semantic(
                Stage5cSemanticBarError::InvalidTimestamp,
            )),
            settlement,
        ));
    };
    if explicit_now_ts_utc_ms < bar_checkpoint_ts_utc_ms {
        return Err(stage5cm_block(
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::Semantic(
                Stage5cSemanticBarError::FutureBar,
            )),
            settlement,
        ));
    }
    if explicit_now
        > recovery_receipt
            .warmup_receipt()
            .restore_receipt()
            .bootstrap_receipt()
            .expires_at()
    {
        return Err(stage5cm_block(
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::Semantic(
                Stage5cSemanticBarError::BrokerTruthExpired,
            )),
            settlement,
        ));
    }
    #[cfg(test)]
    STAGE5GD_R1A_R1_DELEGATE_COUNT.with(|count| count.set(count.get() + 1));
    advance_stage5c_timer_settlement_next_bar_at_checkpoint(
        settlement,
        accepted,
        explicit_now_ts_utc_ms,
        previous_continuation_checkpoint_ts_utc_ms,
    )
}
// STAGE5G-D-R1A-R1-AUTHORITY-END: complete-precallback-transactional-admission-v1

pub fn advance_stage5c_timer_settlement_timer(
    settlement: Stage5cTimerSettlement,
    input: Stage5cPaperTimerInput,
) -> Result<Stage5cTimerResolvedPaperStrategy, Stage5cTimerContinuationFailure> {
    match settlement.inner {
        Stage5cTimerSettlementKind::ReadyForContinuation {
            settled,
            checkpoint_ts_utc_ms,
        } => stage5cm_advance_timer_from_settled(settled, input, checkpoint_ts_utc_ms),
        Stage5cTimerSettlementKind::GeneratedIntentBatch(settled) => Err(stage5cm_block(
            Stage5cTimerContinuationError::GeneratedIntentBatchRequiresLifecycle,
            Stage5cTimerSettlement::generated_intent_batch(settled),
        )),
    }
}

fn stage5ck_block(
    reason: Stage5cPaperTimerError,
    resolved: Stage5cBrokerLifecycleResolvedPaperStrategy,
) -> Stage5cPaperTimerFailure {
    Stage5cPaperTimerFailure::Blocked(Box::new(Stage5cPaperTimerBlocked { reason, resolved }))
}

fn stage5ck_timer_context(
    strategy: &HybridIntradayRuntimeStrategy,
    admission: &Stage5cPaperHostAdmission,
    lifecycle_watermark_ts_utc: i64,
    input: Stage5cPaperTimerInput,
) -> broker_core::HybridRuntimeStrategyContext {
    let timer_ts_utc = input.now_ts_utc_ms.div_euclid(1_000);
    broker_core::HybridRuntimeStrategyContext {
        strategy_id: admission.strategy_id().to_string(),
        request_namespace_account: admission.account_id().clone(),
        instrument: admission.target_instrument().clone(),
        tick_size: admission.tick_size(),
        trade_mode: broker_core::HybridRuntimeTradeMode::Paper,
        paper_execution_mode: broker_core::HybridRuntimePaperExecutionMode::LiveOnly,
        allow_live_orders: false,
        gateway_phase: broker_core::HybridRuntimeGatewayPhase::LiveReady,
        position_qty: Some(strategy.stage5c_current_position_qty()),
        event_ts_utc: timer_ts_utc,
        strategy_now_ts_utc: timer_ts_utc,
        last_bar_ts_utc: Some(lifecycle_watermark_ts_utc),
    }
}

fn stage5ck_lifecycle_watermark_ts_utc(
    batch: &Stage5cPaperIntentBatch,
    ack_outcomes: &[Stage5cPaperAckOutcome],
    event_records: &[Stage5cPaperBrokerEventRecord],
) -> i64 {
    let mut watermark = batch.bar_close_ts();
    for record in &batch.records {
        watermark = watermark.max(record.source_event_ts);
    }
    for ack in ack_outcomes {
        watermark = watermark.max(ack.processed_ts_utc);
    }
    for record in event_records {
        watermark = watermark.max(record.payload.source_ts_utc());
    }
    watermark
}

fn stage5cl_zero_timer_batch(
    strategy: &HybridIntradayRuntimeStrategy,
    recovery_receipt: &Stage5cPendingRecoveryReceipt,
    resolved_batch_summary: &Stage5cPaperIntentBatchSummary,
    timer_ts_utc_ms: i64,
) -> Stage5cPaperIntentBatch {
    let admission = &recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    let timer_ts_utc = timer_ts_utc_ms.div_euclid(1_000);
    Stage5cPaperIntentBatch {
        strategy_id: admission.strategy_id().to_string(),
        account_id: admission.account_id().clone(),
        instrument: admission.target_instrument().clone(),
        bar_close_ts: timer_ts_utc,
        state_fingerprint: stage5c_state_fingerprint(Strategy::state(strategy)),
        request_ids: Vec::new(),
        records: Vec::new(),
        observation_only: resolved_batch_summary.observation_only,
    }
}

fn stage5cm_block(
    reason: Stage5cTimerContinuationError,
    settlement: Stage5cTimerSettlement,
) -> Stage5cTimerContinuationFailure {
    Stage5cTimerContinuationFailure::Blocked(Box::new(Stage5cTimerContinuationBlocked {
        reason,
        settlement,
    }))
}

fn stage5cm_advance_timer_from_settled(
    settled: Stage5cSettledPaperStrategy,
    input: Stage5cPaperTimerInput,
    checkpoint_ts_utc_ms: i64,
) -> Result<Stage5cTimerResolvedPaperStrategy, Stage5cTimerContinuationFailure> {
    if settled.batch.intent_count() > 0 {
        return Err(stage5cm_block(
            Stage5cTimerContinuationError::GeneratedIntentBatchRequiresLifecycle,
            Stage5cTimerSettlement::generated_intent_batch(settled),
        ));
    }
    if input.now_ts_utc_ms <= checkpoint_ts_utc_ms {
        return Err(stage5cm_block(
            Stage5cTimerContinuationError::NonMonotonicTimer,
            Stage5cTimerSettlement::ready_for_continuation(settled, checkpoint_ts_utc_ms),
        ));
    }
    let Some(timer_now) = Utc.timestamp_millis_opt(input.now_ts_utc_ms).single() else {
        return Err(stage5cm_block(
            Stage5cTimerContinuationError::BrokerTruthExpired,
            Stage5cTimerSettlement::ready_for_continuation(settled, checkpoint_ts_utc_ms),
        ));
    };
    if timer_now
        > settled
            .recovery_receipt
            .warmup_receipt()
            .restore_receipt()
            .bootstrap_receipt()
            .expires_at()
    {
        return Err(stage5cm_block(
            Stage5cTimerContinuationError::BrokerTruthExpired,
            Stage5cTimerSettlement::ready_for_continuation(settled, checkpoint_ts_utc_ms),
        ));
    }
    let Stage5cSettledPaperStrategy {
        mut strategy,
        recovery_receipt,
        batch,
        mut settled_batch_history,
    } = settled;
    let admission = &recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    let timer_ts_utc = input.now_ts_utc_ms.div_euclid(1_000);
    let cleanup_ledger =
        stage5cj_cleanup_attribution_ledger(Strategy::state(&strategy), admission.strategy_id());
    let context = stage5ck_timer_context(&strategy, admission, batch.bar_close_ts(), input);
    let intents = crate::BrokerNeutralHybridStrategy::on_broker_timer(
        &mut strategy,
        broker_core::HybridRuntimeCallbackInput {
            context,
            payload: broker_core::HybridRuntimeTimerEvent {
                now_ts_utc_ms: input.now_ts_utc_ms,
            },
        },
    )
    .map_err(|_| {
        Stage5cTimerContinuationFailure::Terminal(
            Stage5cTimerContinuationError::CallbackValidationFailed,
        )
    })?;
    let generated_intent_batch = if intents.is_empty() {
        None
    } else {
        let expected_attribution_by_request =
            stage5cj_expected_generated_attribution_by_request_from_ledger(
                admission,
                timer_ts_utc,
                &intents,
                &cleanup_ledger,
            )
            .map_err(|_| {
                Stage5cTimerContinuationFailure::Terminal(
                    Stage5cTimerContinuationError::GeneratedIntentTerminal,
                )
            })?;
        let generated_batch = stage5c_build_paper_intent_batch(
            &strategy,
            admission,
            timer_ts_utc,
            broker_core::HybridRuntimeBarOrigin::Live,
            intents,
            &expected_attribution_by_request,
        )
        .map_err(|_| {
            Stage5cTimerContinuationFailure::Terminal(
                Stage5cTimerContinuationError::GeneratedIntentTerminal,
            )
        })?;
        stage5cj_verify_generated_batch_final_pending_consistency(
            Strategy::state(&strategy),
            &generated_batch,
        )
        .map_err(|_| {
            Stage5cTimerContinuationFailure::Terminal(
                Stage5cTimerContinuationError::GeneratedIntentTerminal,
            )
        })?;
        settled_batch_history.push(stage5ch_batch_summary(&generated_batch));
        Some(generated_batch)
    };
    let resolved_batch_summary = stage5ch_batch_summary(&batch);
    Ok(Stage5cTimerResolvedPaperStrategy {
        strategy,
        recovery_receipt,
        resolved_batch_summary,
        timer_ts_utc_ms: input.now_ts_utc_ms,
        generated_intent_batch,
        settled_batch_history,
    })
}

fn stage5cj_block(
    reason: Stage5cPaperBrokerLifecycleError,
    resolved: Stage5cResolvedPaperIntentBatchStrategy,
) -> Stage5cPaperBrokerLifecycleFailure {
    Stage5cPaperBrokerLifecycleFailure::Blocked(Box::new(Stage5cPaperBrokerLifecycleBlocked {
        reason,
        resolved,
    }))
}

fn stage5cj_merge_generated_batch(
    target: &mut Option<Stage5cPaperIntentBatch>,
    mut next: Stage5cPaperIntentBatch,
) -> Result<(), Stage5cIntentSettlementError> {
    let Some(existing) = target else {
        *target = Some(next);
        return Ok(());
    };
    let mut seen: HashSet<_> = existing.request_ids.iter().copied().collect();
    for request_id in &next.request_ids {
        if !seen.insert(*request_id) {
            return Err(Stage5cIntentSettlementError::DuplicateRequestId);
        }
    }
    existing.bar_close_ts = existing.bar_close_ts.min(next.bar_close_ts);
    existing.request_ids.append(&mut next.request_ids);
    existing.records.append(&mut next.records);
    Ok(())
}

fn stage5cj_verify_generated_batch_final_pending_consistency(
    final_state: &StrategyState,
    generated_batch: &Stage5cPaperIntentBatch,
) -> Result<(), Stage5cIntentSettlementError> {
    for record in &generated_batch.records {
        stage5cg_verify_pending_request_id(final_state, record.intent_class, record.request_id)?;
    }
    Ok(())
}

fn stage5cj_event_identity(
    record: &Stage5cPaperBrokerEventRecord,
) -> Result<String, serde_json::Error> {
    match &record.payload {
        Stage5cPaperBrokerEventPayload::Order(order) => serde_json::to_string(&(
            record.request_id,
            Stage5cPaperBrokerEventKind::Order,
            &order.order_id,
            &order.status,
            order.source_ts_utc,
        )),
        Stage5cPaperBrokerEventPayload::StopOrder(stop) => serde_json::to_string(&(
            record.request_id,
            Stage5cPaperBrokerEventKind::StopOrder,
            &stop.stop_order_id,
            &stop.status,
            stop.source_ts_utc,
        )),
        Stage5cPaperBrokerEventPayload::Position(position) => serde_json::to_string(&(
            record.request_id,
            Stage5cPaperBrokerEventKind::Position,
            position.source_ts_utc,
        )),
    }
}

fn stage5cj_ack_is_terminal(status: broker_core::HybridRuntimeAckStatus) -> bool {
    matches!(
        status,
        broker_core::HybridRuntimeAckStatus::Rejected
            | broker_core::HybridRuntimeAckStatus::Expired
            | broker_core::HybridRuntimeAckStatus::Error
    )
}

fn stage5cj_expected_event_kind(
    intent: &crate::BrokerNeutralHybridIntent,
) -> Stage5cPaperBrokerEventKind {
    use crate::BrokerNeutralHybridIntent as Intent;
    match intent.base_intent() {
        Intent::Market { .. } => Stage5cPaperBrokerEventKind::Position,
        Intent::Place { .. } | Intent::Cancel { .. } | Intent::Replace { .. } => {
            Stage5cPaperBrokerEventKind::Order
        }
        Intent::CreateStopLimit { .. } | Intent::DeleteStopLimit { .. } => {
            Stage5cPaperBrokerEventKind::StopOrder
        }
        Intent::Classified { .. } | Intent::Routed { .. } => {
            unreachable!("base_intent unwraps wrappers")
        }
    }
}

fn stage5cj_allowed_event_kinds(
    intent_record: &Stage5cPaperIntentRecord,
) -> Vec<Stage5cPaperBrokerEventKind> {
    use crate::BrokerNeutralHybridIntent as Intent;
    match intent_record.intent.base_intent() {
        Intent::Market { .. } => vec![Stage5cPaperBrokerEventKind::Position],
        Intent::Place { .. } => match intent_record.intent_class {
            crate::BrokerNeutralHybridIntentClass::Entry
            | crate::BrokerNeutralHybridIntentClass::Exit
            | crate::BrokerNeutralHybridIntentClass::ProtectiveRepair => vec![
                Stage5cPaperBrokerEventKind::Order,
                Stage5cPaperBrokerEventKind::Position,
            ],
            crate::BrokerNeutralHybridIntentClass::CancelCleanup => {
                vec![Stage5cPaperBrokerEventKind::Order]
            }
        },
        Intent::Cancel { .. } | Intent::Replace { .. } => {
            vec![Stage5cPaperBrokerEventKind::Order]
        }
        Intent::CreateStopLimit { .. } => vec![
            Stage5cPaperBrokerEventKind::StopOrder,
            Stage5cPaperBrokerEventKind::Position,
        ],
        Intent::DeleteStopLimit { .. } => vec![Stage5cPaperBrokerEventKind::StopOrder],
        Intent::Classified { .. } | Intent::Routed { .. } => {
            unreachable!("base_intent unwraps wrappers")
        }
    }
}

fn stage5cj_next_expected_event_kind(
    intent_record: &Stage5cPaperIntentRecord,
    events: &[Stage5cPaperBrokerEventRecord],
) -> Stage5cPaperBrokerEventKind {
    if stage5cj_lifecycle_has_execution_order_like_event_before(intent_record, events, None) {
        Stage5cPaperBrokerEventKind::Position
    } else {
        stage5cj_expected_event_kind(&intent_record.intent)
    }
}

fn stage5cj_validate_event_mapping(
    record: &Stage5cPaperBrokerEventRecord,
    ack: &Stage5cPaperAckOutcome,
    intent_record: &Stage5cPaperIntentRecord,
    admission_strategy_id: &str,
    pre_position_qty: f64,
) -> Result<(), Stage5cPaperBrokerLifecycleFailure> {
    let intent = &intent_record.intent;
    match &record.payload {
        Stage5cPaperBrokerEventPayload::Order(order) => {
            if order.request_id != Some(record.request_id) {
                return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::OrderRequestIdMismatch,
                ));
            }
            if let Some(expected) = &ack.broker_order_id {
                if &order.order_id != expected {
                    return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                        Stage5cPaperBrokerLifecycleError::BrokerOrderIdMismatch,
                    ));
                }
            }
            stage5cj_validate_attribution(
                order.attribution.as_ref(),
                admission_strategy_id,
                stage5cj_expected_order_role(intent_record),
                intent_record.expected_attribution.as_ref(),
            )?;
            stage5cj_validate_order_fields(order, intent)?;
        }
        Stage5cPaperBrokerEventPayload::StopOrder(stop) => {
            if let Some(expected) = &ack.broker_order_id {
                if stop.exchange_order_id.as_ref() != Some(expected) {
                    return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                        Stage5cPaperBrokerLifecycleError::BrokerOrderIdMismatch,
                    ));
                }
            }
            stage5cj_validate_attribution(
                stop.attribution.as_ref(),
                admission_strategy_id,
                stage5cj_expected_order_role(intent_record),
                intent_record.expected_attribution.as_ref(),
            )?;
            stage5cj_validate_stop_fields(stop, intent)?;
            if let crate::BrokerNeutralHybridIntent::DeleteStopLimit { order_id, .. } =
                intent.base_intent()
            {
                if &stop.stop_order_id != order_id {
                    return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                        Stage5cPaperBrokerLifecycleError::StopOrderIdMismatch,
                    ));
                }
            }
        }
        Stage5cPaperBrokerEventPayload::Position(position) => match intent.base_intent() {
            crate::BrokerNeutralHybridIntent::Market { .. }
            | crate::BrokerNeutralHybridIntent::Place { .. }
            | crate::BrokerNeutralHybridIntent::CreateStopLimit { .. } => {
                stage5cj_validate_position_transition(
                    intent_record.intent_class,
                    intent,
                    pre_position_qty,
                    position.qty,
                    position.existing,
                )?;
            }
            _ => {
                return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::UnexpectedBrokerEventKind,
                ));
            }
        },
    }
    Ok(())
}

fn stage5cj_validate_position_transition(
    intent_class: crate::BrokerNeutralHybridIntentClass,
    intent: &crate::BrokerNeutralHybridIntent,
    pre_position_qty: f64,
    new_position_qty: f64,
    existing: bool,
) -> Result<(), Stage5cPaperBrokerLifecycleFailure> {
    if !existing {
        return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
            Stage5cPaperBrokerLifecycleError::PositionEventRequiresMarketIntent,
        ));
    }
    match intent_class {
        crate::BrokerNeutralHybridIntentClass::Entry => {
            let Some((side, target_qty)) = stage5cj_entry_side_and_target_qty(intent) else {
                return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::PositionEventRequiresMarketIntent,
                ));
            };
            let signed_ok = match side {
                crate::BrokerNeutralOrderSide::Buy => new_position_qty > f64::EPSILON,
                crate::BrokerNeutralOrderSide::Sell => new_position_qty < -f64::EPSILON,
            };
            if !signed_ok {
                return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::PositionSideMismatch,
                ));
            }
            if new_position_qty.abs() > target_qty.abs() + f64::EPSILON {
                return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::PositionOverfill,
                ));
            }
            if pre_position_qty.abs() > f64::EPSILON
                && new_position_qty.signum() == pre_position_qty.signum()
                && new_position_qty.abs() + f64::EPSILON < pre_position_qty.abs()
            {
                return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::PositionRegression,
                ));
            }
            Ok(())
        }
        crate::BrokerNeutralHybridIntentClass::Exit => {
            if pre_position_qty.abs() > f64::EPSILON && new_position_qty.abs() <= f64::EPSILON {
                Ok(())
            } else {
                Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::PositionEventRequiresMarketIntent,
                ))
            }
        }
        crate::BrokerNeutralHybridIntentClass::ProtectiveRepair => {
            if new_position_qty.abs() <= f64::EPSILON {
                Ok(())
            } else {
                Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::PositionEventRequiresMarketIntent,
                ))
            }
        }
        crate::BrokerNeutralHybridIntentClass::CancelCleanup => {
            Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::PositionEventRequiresMarketIntent,
            ))
        }
    }
}

fn stage5cj_entry_side_and_target_qty(
    intent: &crate::BrokerNeutralHybridIntent,
) -> Option<(crate::BrokerNeutralOrderSide, f64)> {
    match intent.base_intent() {
        crate::BrokerNeutralHybridIntent::Market { side, qty, .. }
        | crate::BrokerNeutralHybridIntent::Place { side, qty, .. } => Some((*side, *qty)),
        _ => None,
    }
}

fn stage5cj_validate_attribution(
    attribution: Option<&broker_core::HybridRuntimeAttribution>,
    admission_strategy_id: &str,
    expected_role: Option<broker_core::HybridRuntimeOrderRole>,
    expected_attribution: Option<&broker_core::HybridRuntimeAttribution>,
) -> Result<(), Stage5cPaperBrokerLifecycleFailure> {
    let Some(attribution) = attribution else {
        return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
            Stage5cPaperBrokerLifecycleError::AttributionMissing,
        ));
    };
    if !attribution.belongs_to(admission_strategy_id) {
        return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
            Stage5cPaperBrokerLifecycleError::AttributionStrategyMismatch,
        ));
    }
    if expected_role.is_some() && attribution.role() != expected_role {
        return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
            Stage5cPaperBrokerLifecycleError::AttributionRoleMismatch,
        ));
    }
    if let Some(expected) = expected_attribution {
        if attribution.cycle_id() != expected.cycle_id()
            || attribution.owner() != expected.owner()
            || attribution.role() != expected.role()
        {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::AttributionCycleMismatch,
            ));
        }
    }
    Ok(())
}

fn stage5cj_expected_order_role(
    intent_record: &Stage5cPaperIntentRecord,
) -> Option<broker_core::HybridRuntimeOrderRole> {
    match intent_record.intent.base_intent() {
        crate::BrokerNeutralHybridIntent::Place { .. } => match intent_record.intent_class {
            crate::BrokerNeutralHybridIntentClass::Entry => {
                Some(broker_core::HybridRuntimeOrderRole::Entry)
            }
            crate::BrokerNeutralHybridIntentClass::Exit => {
                Some(broker_core::HybridRuntimeOrderRole::Exit)
            }
            crate::BrokerNeutralHybridIntentClass::ProtectiveRepair => {
                Some(broker_core::HybridRuntimeOrderRole::TakeProfit)
            }
            crate::BrokerNeutralHybridIntentClass::CancelCleanup => {
                Some(broker_core::HybridRuntimeOrderRole::Cancel)
            }
        },
        crate::BrokerNeutralHybridIntent::CreateStopLimit { .. } => {
            Some(broker_core::HybridRuntimeOrderRole::StopLoss)
        }
        crate::BrokerNeutralHybridIntent::Cancel { .. }
        | crate::BrokerNeutralHybridIntent::DeleteStopLimit { .. } => intent_record
            .expected_attribution
            .as_ref()
            .and_then(broker_core::HybridRuntimeAttribution::role),
        crate::BrokerNeutralHybridIntent::Replace { .. } => {
            Some(broker_core::HybridRuntimeOrderRole::TakeProfit)
        }
        crate::BrokerNeutralHybridIntent::Market { .. }
        | crate::BrokerNeutralHybridIntent::Classified { .. }
        | crate::BrokerNeutralHybridIntent::Routed { .. } => None,
    }
}

fn stage5cj_expected_attribution_for_intent(
    state: &StrategyState,
    strategy_id: &str,
    intent_class: crate::BrokerNeutralHybridIntentClass,
    intent: &crate::BrokerNeutralHybridIntent,
) -> Option<broker_core::HybridRuntimeAttribution> {
    if let Some(comment) = stage5cj_expected_comment(intent) {
        return broker_core::HybridRuntimeAttribution::parse_source_comment(comment).ok();
    }
    match (state, intent.base_intent()) {
        (
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                current_owner,
                tp_order_id,
                sl_stop_order_id,
                sl_exchange_order_id,
                ..
            },
            crate::BrokerNeutralHybridIntent::Cancel { order_id },
        ) if intent_class == crate::BrokerNeutralHybridIntentClass::CancelCleanup => {
            let role = if tp_order_id.as_ref() == Some(order_id) {
                Some(broker_core::HybridRuntimeOrderRole::TakeProfit)
            } else if sl_exchange_order_id.as_ref() == Some(order_id) {
                Some(broker_core::HybridRuntimeOrderRole::StopLoss)
            } else {
                None
            }?;
            stage5cj_build_expected_attribution(strategy_id, active_cycle_id, current_owner, role)
        }
        (
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                current_owner,
                sl_stop_order_id,
                ..
            },
            crate::BrokerNeutralHybridIntent::DeleteStopLimit { order_id, .. },
        ) if intent_class == crate::BrokerNeutralHybridIntentClass::CancelCleanup
            && sl_stop_order_id.as_ref() == Some(order_id) =>
        {
            stage5cj_build_expected_attribution(
                strategy_id,
                active_cycle_id,
                current_owner,
                broker_core::HybridRuntimeOrderRole::StopLoss,
            )
        }
        _ => None,
    }
}

fn stage5cj_cleanup_attribution_ledger(
    state: &StrategyState,
    strategy_id: &str,
) -> Stage5cCleanupAttributionLedger {
    let mut ledger = Stage5cCleanupAttributionLedger::default();
    let StrategyState::HybridIntradayRuntime {
        active_cycle_id,
        current_owner,
        tp_order_id,
        sl_stop_order_id,
        sl_exchange_order_id,
        pending_entry_owner,
        pending_entry_cycle_id,
        ..
    } = state
    else {
        return ledger;
    };
    if let Some(attribution) = stage5cj_build_expected_attribution(
        strategy_id,
        active_cycle_id,
        current_owner,
        broker_core::HybridRuntimeOrderRole::TakeProfit,
    ) {
        if let Some(order_id) = tp_order_id {
            ledger.broker_orders.insert(order_id.clone(), attribution);
        }
    }
    if let Some(attribution) = stage5cj_build_expected_attribution(
        strategy_id,
        active_cycle_id,
        current_owner,
        broker_core::HybridRuntimeOrderRole::StopLoss,
    ) {
        if let Some(order_id) = sl_exchange_order_id {
            ledger
                .broker_orders
                .insert(order_id.clone(), attribution.clone());
        }
        if let Some(stop_order_id) = sl_stop_order_id {
            ledger
                .stop_orders
                .insert(stop_order_id.clone(), attribution);
        }
    }
    ledger.pending_entry_attribution = stage5cj_build_expected_attribution(
        strategy_id,
        pending_entry_cycle_id,
        pending_entry_owner,
        broker_core::HybridRuntimeOrderRole::Entry,
    );
    ledger
}

fn stage5cj_expected_cleanup_attribution_from_ledger(
    ledger: &Stage5cCleanupAttributionLedger,
    intent: &crate::BrokerNeutralHybridIntent,
) -> Option<broker_core::HybridRuntimeAttribution> {
    match intent.base_intent() {
        crate::BrokerNeutralHybridIntent::Cancel { order_id } => ledger
            .broker_orders
            .get(order_id)
            .cloned()
            .or_else(|| ledger.pending_entry_attribution.clone()),
        crate::BrokerNeutralHybridIntent::DeleteStopLimit { order_id, .. } => {
            ledger.stop_orders.get(order_id).cloned()
        }
        _ => None,
    }
}

fn stage5cj_expected_generated_attribution_by_request_from_ledger(
    admission: &Stage5cPaperHostAdmission,
    source_ts: i64,
    intents: &[crate::BrokerNeutralHybridIntent],
    ledger: &Stage5cCleanupAttributionLedger,
) -> Result<
    HashMap<StrategyRequestId, broker_core::HybridRuntimeAttribution>,
    Stage5cIntentSettlementError,
> {
    let mut expected = HashMap::new();
    let mut seen_request_ids = HashSet::new();
    for intent in intents {
        let request_id = stage5cg_source_request_id(
            admission.strategy_id(),
            admission.account_id().as_str(),
            &admission.target_instrument().symbol,
            source_ts,
            intent,
        )?;
        if !seen_request_ids.insert(request_id) {
            return Err(Stage5cIntentSettlementError::DuplicateRequestId);
        }
        if let Some(attribution) = stage5cj_expected_cleanup_attribution_from_ledger(ledger, intent)
        {
            expected.insert(request_id, attribution);
        }
    }
    Ok(expected)
}

fn stage5cj_build_expected_attribution(
    strategy_id: &str,
    active_cycle_id: &Option<String>,
    current_owner: &Option<crate::hybrid_intraday::Owner>,
    role: broker_core::HybridRuntimeOrderRole,
) -> Option<broker_core::HybridRuntimeAttribution> {
    let cycle = active_cycle_id.as_deref()?;
    let owner = match current_owner.as_ref()? {
        crate::hybrid_intraday::Owner::MeanReversion => "MR",
        crate::hybrid_intraday::Owner::IntradayBreakout => "BO",
    };
    let role = match role {
        broker_core::HybridRuntimeOrderRole::Entry => "ENTRY",
        broker_core::HybridRuntimeOrderRole::Exit => "EXIT",
        broker_core::HybridRuntimeOrderRole::TakeProfit => "TP",
        broker_core::HybridRuntimeOrderRole::StopLoss => "SL",
        broker_core::HybridRuntimeOrderRole::Cancel => "CANCEL",
        broker_core::HybridRuntimeOrderRole::Repair => "REPAIR",
    };
    broker_core::HybridRuntimeAttribution::parse_source_comment(format!(
        "HYB|sid={strategy_id}|c={cycle}|o={owner}|r={role}"
    ))
    .ok()
}

fn stage5cj_expected_comment(intent: &crate::BrokerNeutralHybridIntent) -> Option<&str> {
    match intent.base_intent() {
        crate::BrokerNeutralHybridIntent::Place { comment, .. }
        | crate::BrokerNeutralHybridIntent::Market { comment, .. }
        | crate::BrokerNeutralHybridIntent::CreateStopLimit { comment, .. } => comment.as_deref(),
        _ => None,
    }
}

fn stage5cj_validate_order_fields(
    order: &broker_core::HybridRuntimeOrderEvent,
    intent: &crate::BrokerNeutralHybridIntent,
) -> Result<(), Stage5cPaperBrokerLifecycleFailure> {
    if !stage5cj_order_status_is_known(&order.status) {
        return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
            Stage5cPaperBrokerLifecycleError::UnknownOrderStatus,
        ));
    }
    match intent.base_intent() {
        crate::BrokerNeutralHybridIntent::Place {
            price, qty, side, ..
        } if !stage5cj_side_matches(&order.side, *side)
            || !stage5cj_f64_eq(order.qty, *qty)
            || !stage5cj_f64_eq(order.price, *price)
            || !order.order_type.eq_ignore_ascii_case("limit") =>
        {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::IntentFieldMismatch,
            ));
        }
        crate::BrokerNeutralHybridIntent::Cancel { order_id } => {
            if &order.order_id != order_id {
                return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::BrokerOrderIdMismatch,
                ));
            }
            if !stage5cj_order_status_is_cancel_terminal(&order.status)
                && !stage5cj_order_status_is_working(&order.status)
            {
                return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::IntentFieldMismatch,
                ));
            }
        }
        crate::BrokerNeutralHybridIntent::Replace {
            order_id,
            new_price,
            new_qty,
        } if &order.order_id != order_id
            || !stage5cj_f64_eq(order.price, *new_price)
            || !stage5cj_f64_eq(order.qty, *new_qty) =>
        {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::IntentFieldMismatch,
            ));
        }
        _ => {}
    }
    Ok(())
}

fn stage5cj_validate_stop_fields(
    stop: &broker_core::HybridRuntimeStopOrderEvent,
    intent: &crate::BrokerNeutralHybridIntent,
) -> Result<(), Stage5cPaperBrokerLifecycleFailure> {
    if !stage5cj_stop_status_is_known(&stop.status) {
        return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
            Stage5cPaperBrokerLifecycleError::UnknownStopOrderStatus,
        ));
    }
    match intent.base_intent() {
        crate::BrokerNeutralHybridIntent::CreateStopLimit {
            side,
            qty,
            trigger_price,
            price,
            stop_end_unix_time,
            ..
        } if !stage5cj_side_matches(&stop.side, *side)
            || !stage5cj_f64_eq(stop.qty, *qty)
            || !stage5cj_f64_eq(stop.stop_price, *trigger_price)
            || !stage5cj_f64_eq(stop.price, *price)
            || stop.end_ts_utc != Some(*stop_end_unix_time) =>
        {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::IntentFieldMismatch,
            ));
        }
        crate::BrokerNeutralHybridIntent::DeleteStopLimit { order_id, side, .. }
            if &stop.stop_order_id != order_id
                || side.is_some_and(|expected| !stage5cj_side_matches(&stop.side, expected)) =>
        {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::IntentFieldMismatch,
            ));
        }
        _ => {}
    }
    Ok(())
}

fn stage5cj_event_is_terminal_for_intent(
    record: &Stage5cPaperBrokerEventRecord,
    intent_record: &Stage5cPaperIntentRecord,
    events: &[Stage5cPaperBrokerEventRecord],
) -> bool {
    match (&record.payload, intent_record.intent.base_intent()) {
        (
            Stage5cPaperBrokerEventPayload::Position(position),
            crate::BrokerNeutralHybridIntent::Market { qty, .. },
        ) => stage5cj_market_position_is_terminal(intent_record.intent_class, *qty, position.qty),
        (
            Stage5cPaperBrokerEventPayload::Position(position),
            crate::BrokerNeutralHybridIntent::Place { .. },
        ) => {
            stage5cj_lifecycle_has_execution_order_like_event_before(
                intent_record,
                events,
                Some(record.total_sequence),
            ) && stage5cj_position_confirms_place_terminal(
                intent_record.intent_class,
                &intent_record.intent,
                position.qty,
            )
        }
        (
            Stage5cPaperBrokerEventPayload::Position(position),
            crate::BrokerNeutralHybridIntent::CreateStopLimit { .. },
        ) => {
            stage5cj_lifecycle_has_execution_order_like_event_before(
                intent_record,
                events,
                Some(record.total_sequence),
            ) && position.qty.abs() <= f64::EPSILON
        }
        (
            Stage5cPaperBrokerEventPayload::Order(order),
            crate::BrokerNeutralHybridIntent::Place { .. },
        ) => stage5cj_order_status_is_cancel_terminal(&order.status),
        (
            Stage5cPaperBrokerEventPayload::Order(order),
            crate::BrokerNeutralHybridIntent::Cancel { .. },
        ) => stage5cj_order_status_is_cancel_terminal(&order.status),
        (
            Stage5cPaperBrokerEventPayload::Order(order),
            crate::BrokerNeutralHybridIntent::Replace { .. },
        ) => stage5cj_order_status_is_known(&order.status),
        (
            Stage5cPaperBrokerEventPayload::StopOrder(stop),
            crate::BrokerNeutralHybridIntent::DeleteStopLimit { .. },
        ) => stage5cj_stop_status_is_terminal(&stop.status),
        (
            Stage5cPaperBrokerEventPayload::StopOrder(stop),
            crate::BrokerNeutralHybridIntent::CreateStopLimit { .. },
        ) => stage5cj_stop_status_is_non_execution_terminal(&stop.status),
        _ => false,
    }
}

fn stage5cj_lifecycle_has_execution_order_like_event_before(
    intent_record: &Stage5cPaperIntentRecord,
    events: &[Stage5cPaperBrokerEventRecord],
    before_total_sequence: Option<u64>,
) -> bool {
    events.iter().any(
        |event| match (&event.payload, intent_record.intent.base_intent()) {
            _ if before_total_sequence.is_some_and(|before| event.total_sequence >= before) => {
                false
            }
            (
                Stage5cPaperBrokerEventPayload::Order(order),
                crate::BrokerNeutralHybridIntent::Place { .. },
            ) => stage5cj_order_status_is_filled(&order.status),
            (
                Stage5cPaperBrokerEventPayload::StopOrder(stop),
                crate::BrokerNeutralHybridIntent::CreateStopLimit { .. },
            ) => stage5cj_stop_status_is_execution(&stop.status),
            _ => false,
        },
    )
}

fn stage5cj_market_position_is_terminal(
    intent_class: crate::BrokerNeutralHybridIntentClass,
    target_qty: f64,
    position_qty: f64,
) -> bool {
    match intent_class {
        crate::BrokerNeutralHybridIntentClass::Entry => {
            position_qty.abs() + f64::EPSILON >= target_qty.abs()
        }
        crate::BrokerNeutralHybridIntentClass::Exit => position_qty.abs() <= f64::EPSILON,
        crate::BrokerNeutralHybridIntentClass::ProtectiveRepair
        | crate::BrokerNeutralHybridIntentClass::CancelCleanup => false,
    }
}

fn stage5cj_position_confirms_place_terminal(
    intent_class: crate::BrokerNeutralHybridIntentClass,
    intent: &crate::BrokerNeutralHybridIntent,
    position_qty: f64,
) -> bool {
    match intent_class {
        crate::BrokerNeutralHybridIntentClass::Entry => match intent.base_intent() {
            crate::BrokerNeutralHybridIntent::Place { qty, .. } => {
                position_qty.abs() + f64::EPSILON >= qty.abs()
            }
            _ => false,
        },
        crate::BrokerNeutralHybridIntentClass::Exit
        | crate::BrokerNeutralHybridIntentClass::ProtectiveRepair => {
            position_qty.abs() <= f64::EPSILON
        }
        crate::BrokerNeutralHybridIntentClass::CancelCleanup => false,
    }
}

fn stage5cj_order_status_is_working(status: &str) -> bool {
    matches!(
        status.to_ascii_lowercase().as_str(),
        "working" | "active" | "accepted" | "new" | "partially_filled" | "partial"
    )
}

fn stage5cj_order_status_is_filled(status: &str) -> bool {
    matches!(
        status.to_ascii_lowercase().as_str(),
        "filled" | "done" | "completed"
    )
}

fn stage5cj_order_status_is_cancel_terminal(status: &str) -> bool {
    matches!(
        status.to_ascii_lowercase().as_str(),
        "canceled" | "cancelled" | "expired" | "rejected"
    )
}

fn stage5cj_order_status_is_known(status: &str) -> bool {
    stage5cj_order_status_is_working(status)
        || stage5cj_order_status_is_filled(status)
        || stage5cj_order_status_is_cancel_terminal(status)
}

fn stage5cj_stop_status_is_terminal(status: &str) -> bool {
    stage5cj_stop_status_is_execution(status)
        || stage5cj_stop_status_is_non_execution_terminal(status)
}

fn stage5cj_stop_status_is_execution(status: &str) -> bool {
    matches!(
        status.to_ascii_lowercase().as_str(),
        "triggered" | "filled" | "executed" | "done" | "completed"
    )
}

fn stage5cj_stop_status_is_non_execution_terminal(status: &str) -> bool {
    matches!(
        status.to_ascii_lowercase().as_str(),
        "canceled" | "cancelled" | "expired" | "rejected"
    )
}

fn stage5cj_stop_status_is_known(status: &str) -> bool {
    matches!(
        status.to_ascii_lowercase().as_str(),
        "working" | "active" | "accepted" | "new"
    ) || stage5cj_stop_status_is_terminal(status)
}

fn stage5cj_side_matches(actual: &str, expected: crate::BrokerNeutralOrderSide) -> bool {
    matches!(
        (actual.to_ascii_lowercase().as_str(), expected),
        ("buy", crate::BrokerNeutralOrderSide::Buy) | ("sell", crate::BrokerNeutralOrderSide::Sell)
    )
}

fn stage5cj_f64_eq(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1e-9
}

fn stage5cj_position_qty(state: &StrategyState) -> f64 {
    match state {
        StrategyState::HybridIntradayRuntime {
            last_position_qty, ..
        } => *last_position_qty,
        StrategyState::Idle => 0.0,
    }
}

fn stage5cj_broker_lifecycle_context(
    strategy: &HybridIntradayRuntimeStrategy,
    admission: &Stage5cPaperHostAdmission,
    bar_close_ts: i64,
    event_ts_utc: i64,
) -> broker_core::HybridRuntimeStrategyContext {
    broker_core::HybridRuntimeStrategyContext {
        strategy_id: admission.strategy_id().to_string(),
        request_namespace_account: admission.account_id().clone(),
        instrument: admission.target_instrument().clone(),
        tick_size: admission.tick_size(),
        trade_mode: broker_core::HybridRuntimeTradeMode::Paper,
        paper_execution_mode: broker_core::HybridRuntimePaperExecutionMode::LiveOnly,
        allow_live_orders: false,
        gateway_phase: broker_core::HybridRuntimeGatewayPhase::LiveReady,
        position_qty: Some(strategy.stage5c_current_position_qty()),
        event_ts_utc,
        strategy_now_ts_utc: event_ts_utc,
        last_bar_ts_utc: Some(bar_close_ts),
    }
}

// STAGE5G-C-R2CA-R1-AUTHORITY-BEGIN: market-terminal-state-coherence-v1
/// Canonical broker evidence for an ACK-accepted or ACK-confirmed MARKET intent
/// that later reaches a broker terminal status.
///
/// This input is crate-private and non-serializable. Validation consumes it and
/// issues an opaque single-use capability; it cannot itself mint timer-ready
/// lifecycle state.
#[allow(dead_code)]
pub(crate) struct Stage5cMarketTerminalOrderEvidence {
    pub(crate) request_id: StrategyRequestId,
    pub(crate) truth: broker_core::BrokerTruthSnapshot,
    pub(crate) attribution: Option<broker_core::HybridRuntimeAttribution>,
}

#[allow(dead_code)]
struct Stage5cValidatedMarketTerminalFacts {
    request_id: StrategyRequestId,
    ack_status: broker_core::HybridRuntimeAckStatus,
    account_id: BrokerAccountId,
    broker_order_id: BrokerOrderId,
    client_order_id: broker_core::ClientOrderId,
    instrument: broker_core::InstrumentId,
    side: crate::BrokerNeutralOrderSide,
    attribution: broker_core::HybridRuntimeAttribution,
    order_status: broker_core::OrderStatus,
    order_qty: rust_decimal::Decimal,
    filled_qty: rust_decimal::Decimal,
    target_position_qty: rust_decimal::Decimal,
    target_avg_price: rust_decimal::Decimal,
    intent_class: crate::BrokerNeutralHybridIntentClass,
    lifecycle_event_ts_utc: i64,
    lifecycle_watermark_ts_utc: i64,
    broker_event_count: usize,
    evidence_fingerprint: String,
    correlated_trades: Vec<broker_core::BrokerTradeSnapshot>,
    target_positions: Vec<broker_core::BrokerPositionSnapshot>,
}

/// Validation-only authority. It owns the original retry capability and exact
/// canonical facts, but deliberately exposes no timer/continuation interface.
#[allow(dead_code)]
pub(crate) struct Stage5cValidatedMarketTerminalOutcome {
    resolved: Stage5cResolvedPaperIntentBatchStrategy,
    facts: Stage5cValidatedMarketTerminalFacts,
}

#[allow(dead_code)]
impl Stage5cValidatedMarketTerminalOutcome {
    #[cfg(test)]
    fn evidence_fingerprint(&self) -> &str {
        &self.facts.evidence_fingerprint
    }
}

/// Validates all broker truth before any strategy mutation. On failure, the
/// exact original resolved capability is returned for corrected reconciliation.
#[allow(dead_code)]
pub(crate) fn validate_stage5c_market_terminal_outcome(
    resolved: Stage5cResolvedPaperIntentBatchStrategy,
    evidence: Stage5cMarketTerminalOrderEvidence,
) -> Result<Stage5cValidatedMarketTerminalOutcome, Stage5cPaperBrokerLifecycleFailure> {
    let facts = match stage5c_validate_market_terminal_order(&resolved, &evidence) {
        Ok(facts) => facts,
        Err(reason) => return Err(stage5cj_block(reason, resolved)),
    };
    Ok(Stage5cValidatedMarketTerminalOutcome { resolved, facts })
}

/// Applies a validated MARKET terminal outcome through the mature runtime ACK
/// and position transitions. Zero-fill outcomes clear stale pending state;
/// positive fills update exact broker position and retain generated recovery
/// intents in the existing Stage 5C escrow.
#[allow(dead_code)]
pub(crate) fn settle_stage5c_validated_market_terminal_outcome(
    validated: Stage5cValidatedMarketTerminalOutcome,
) -> Result<Stage5cBrokerLifecycleSettlement, Stage5cPaperBrokerLifecycleFailure> {
    let Stage5cValidatedMarketTerminalOutcome { resolved, facts } = validated;
    let Stage5cResolvedPaperIntentBatchStrategy {
        mut strategy,
        recovery_receipt,
        resolved_batch,
        ack_outcomes,
        mut settled_batch_history,
    } = resolved;
    let admission = &recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    let cleanup_ledger =
        stage5cj_cleanup_attribution_ledger(Strategy::state(&strategy), admission.strategy_id());
    let position_qty =
        facts
            .target_position_qty
            .to_f64()
            .ok_or(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::IntentFieldMismatch,
            ))?;
    let avg_price =
        facts
            .target_avg_price
            .to_f64()
            .ok_or(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::IntentFieldMismatch,
            ))?;
    let is_positive_fill = facts.filled_qty > rust_decimal::Decimal::ZERO;
    let is_full_fill = is_positive_fill && facts.filled_qty == facts.order_qty;
    let mut generated_intents = Vec::new();

    if is_full_fill {
        generated_intents.extend(stage5c_apply_market_terminal_position(
            &mut strategy,
            admission,
            &resolved_batch,
            position_qty,
            avg_price,
            facts.lifecycle_event_ts_utc,
            true,
        )?);
    }

    let terminal_ack = stage5c_market_terminal_runtime_ack(&facts);
    let terminal_ack_intents =
        crate::BrokerNeutralHybridStrategy::on_broker_ack(&mut strategy, terminal_ack).map_err(
            |_| {
                Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::CallbackValidationFailed,
                )
            },
        )?;
    if !terminal_ack_intents.is_empty() {
        return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
            Stage5cPaperBrokerLifecycleError::CallbackGeneratedIntentTerminal,
        ));
    }

    if is_positive_fill && !is_full_fill {
        generated_intents.extend(stage5c_apply_market_terminal_position(
            &mut strategy,
            admission,
            &resolved_batch,
            position_qty,
            avg_price,
            facts.lifecycle_event_ts_utc,
            false,
        )?);
        if generated_intents.is_empty() {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::CallbackGeneratedIntentTerminal,
            ));
        }
    }

    if !stage5c_market_terminal_state_is_coherent(
        Strategy::state(&strategy),
        facts.request_id,
        facts.intent_class,
        position_qty,
    ) {
        return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
            Stage5cPaperBrokerLifecycleError::CallbackValidationFailed,
        ));
    }

    let mut generated_intent_batch = None;
    if !generated_intents.is_empty() {
        let expected_attribution_by_request =
            stage5cj_expected_generated_attribution_by_request_from_ledger(
                admission,
                facts.lifecycle_event_ts_utc,
                &generated_intents,
                &cleanup_ledger,
            )
            .map_err(|_| {
                Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::CallbackGeneratedIntentTerminal,
                )
            })?;
        let mut callback_batch = stage5c_build_paper_intent_batch(
            &strategy,
            admission,
            facts.lifecycle_event_ts_utc,
            broker_core::HybridRuntimeBarOrigin::Live,
            generated_intents,
            &expected_attribution_by_request,
        )
        .map_err(|_| {
            Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::CallbackGeneratedIntentTerminal,
            )
        })?;
        stage5cj_verify_generated_batch_final_pending_consistency(
            Strategy::state(&strategy),
            &callback_batch,
        )
        .map_err(|_| {
            Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::CallbackGeneratedIntentTerminal,
            )
        })?;
        callback_batch.state_fingerprint = stage5c_state_fingerprint(Strategy::state(&strategy));
        settled_batch_history.push(stage5ch_batch_summary(&callback_batch));
        generated_intent_batch = Some(callback_batch);
    }

    let resolved_batch_summary = stage5ch_batch_summary(&resolved_batch);
    let resolved = Stage5cBrokerLifecycleResolvedPaperStrategy {
        strategy,
        recovery_receipt,
        resolved_batch,
        resolved_batch_summary,
        ack_outcomes,
        broker_event_count: facts.broker_event_count,
        remaining_lifecycle_expectations: Vec::new(),
        lifecycle_watermark_ts_utc: facts.lifecycle_watermark_ts_utc,
        generated_intent_batch,
        settled_batch_history,
    };
    Ok(settle_stage5c_broker_lifecycle_result(resolved))
}

#[allow(dead_code)]
fn stage5c_validate_market_terminal_order(
    resolved: &Stage5cResolvedPaperIntentBatchStrategy,
    evidence: &Stage5cMarketTerminalOrderEvidence,
) -> Result<Stage5cValidatedMarketTerminalFacts, Stage5cPaperBrokerLifecycleError> {
    if resolved.resolved_batch.records.len() != 1 || resolved.ack_outcomes.len() != 1 {
        return Err(Stage5cPaperBrokerLifecycleError::IntentFieldMismatch);
    }
    let record = &resolved.resolved_batch.records[0];
    let ack = &resolved.ack_outcomes[0];
    if record.request_id != evidence.request_id
        || ack.request_id != evidence.request_id
        || !matches!(
            ack.status,
            broker_core::HybridRuntimeAckStatus::Accepted
                | broker_core::HybridRuntimeAckStatus::Confirmed
        )
    {
        return Err(Stage5cPaperBrokerLifecycleError::OrderRequestIdMismatch);
    }
    let broker_order_id = ack
        .broker_order_id
        .as_ref()
        .ok_or(Stage5cPaperBrokerLifecycleError::BrokerOrderIdMismatch)?;
    let expected_client_order_id =
        broker_core::ClientOrderId::from_strategy_request(evidence.request_id);
    let (expected_qty, expected_side) = match record.intent.base_intent() {
        crate::BrokerNeutralHybridIntent::Market { qty, side, .. }
            if qty.is_finite() && *qty > 0.0 =>
        {
            (*qty, *side)
        }
        _ => return Err(Stage5cPaperBrokerLifecycleError::IntentFieldMismatch),
    };
    let expected_attribution = record
        .expected_attribution
        .as_ref()
        .ok_or(Stage5cPaperBrokerLifecycleError::AttributionMissing)?;
    if evidence.attribution.as_ref() != Some(expected_attribution) {
        return Err(Stage5cPaperBrokerLifecycleError::AttributionRoleMismatch);
    }

    let admission = &resolved
        .recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    if evidence.truth.account_id != *admission.account_id() {
        return Err(Stage5cPaperBrokerLifecycleError::InstrumentMismatch);
    }
    if evidence.truth.received_ts.timestamp() < ack.processed_ts_utc {
        return Err(Stage5cPaperBrokerLifecycleError::EventTimestampBeforeAck);
    }
    let target = admission.target_instrument();
    let matching_orders: Vec<_> = evidence
        .truth
        .orders
        .iter()
        .filter(|order| {
            order.broker_order_id.as_ref() == Some(broker_order_id)
                || order.client_order_id.as_ref() == Some(&expected_client_order_id)
        })
        .collect();
    if matching_orders.len() != 1 {
        return Err(Stage5cPaperBrokerLifecycleError::BrokerOrderIdMismatch);
    }
    let order = matching_orders[0];
    if order.account_id != *admission.account_id()
        || order.broker_order_id.as_ref() != Some(broker_order_id)
        || order.client_order_id.as_ref() != Some(&expected_client_order_id)
        || !broker_core::instrument_identity_matches(&order.instrument, target)
        || order.order_type != broker_core::OrderType::Market
        || !stage5c_order_side_matches(order.side, expected_side)
        || order.qty
            != rust_decimal::Decimal::from_f64_retain(expected_qty)
                .ok_or(Stage5cPaperBrokerLifecycleError::IntentFieldMismatch)?
        || order.filled_qty < rust_decimal::Decimal::ZERO
        || order.remaining_qty != Some(order.qty - order.filled_qty)
        || order.lifecycle != broker_core::BrokerOrderLifecycle::Terminal
        || order.source_ts.map(|ts| ts.timestamp()).unwrap_or_default() < ack.processed_ts_utc
        || order
            .source_ts
            .is_some_and(|source_ts| source_ts > order.received_ts)
        || order.received_ts > evidence.truth.received_ts
    {
        return Err(Stage5cPaperBrokerLifecycleError::IntentFieldMismatch);
    }
    if !matches!(
        order.status,
        broker_core::OrderStatus::Rejected
            | broker_core::OrderStatus::Canceled
            | broker_core::OrderStatus::Expired
    ) {
        return Err(Stage5cPaperBrokerLifecycleError::UnknownOrderStatus);
    }

    let mut trade_ids = HashSet::new();
    let mut correlated_trades = Vec::new();
    let mut correlated_trade_qty = rust_decimal::Decimal::ZERO;
    let mut correlated_trade_count = 0usize;
    for trade in evidence.truth.trades.iter().filter(|trade| {
        trade.broker_order_id.as_ref() == Some(broker_order_id)
            || trade.client_order_id.as_ref() == Some(&expected_client_order_id)
    }) {
        if !trade_ids.insert(trade.broker_trade_id.clone()) {
            return Err(Stage5cPaperBrokerLifecycleError::ConflictingDuplicateEvent);
        }
        if trade.account_id != *admission.account_id()
            || trade.broker_order_id.as_ref() != Some(broker_order_id)
            || trade.client_order_id.as_ref() != Some(&expected_client_order_id)
            || !broker_core::instrument_identity_matches(&trade.instrument, target)
            || !stage5c_order_side_matches(trade.side, expected_side)
            || trade.qty <= rust_decimal::Decimal::ZERO
            || trade.source_ts.timestamp() < ack.processed_ts_utc
            || trade.source_ts > trade.received_ts
            || trade.received_ts > evidence.truth.received_ts
        {
            return Err(Stage5cPaperBrokerLifecycleError::IntentFieldMismatch);
        }
        correlated_trade_qty += trade.qty;
        correlated_trade_count += 1;
        correlated_trades.push(trade.clone());
    }
    if correlated_trade_qty != order.filled_qty || order.filled_qty > order.qty {
        return Err(Stage5cPaperBrokerLifecycleError::PositionOverfill);
    }
    if order.status == broker_core::OrderStatus::Rejected
        && order.filled_qty != rust_decimal::Decimal::ZERO
    {
        return Err(Stage5cPaperBrokerLifecycleError::IntentFieldMismatch);
    }

    let mut target_position_qty = rust_decimal::Decimal::ZERO;
    let mut target_position_value = rust_decimal::Decimal::ZERO;
    let mut target_position_weight = rust_decimal::Decimal::ZERO;
    let mut target_positions = Vec::new();
    for position in
        evidence.truth.positions.iter().filter(|position| {
            broker_core::instrument_identity_matches(&position.instrument, target)
        })
    {
        let position_source_ts = position
            .source_ts
            .ok_or(Stage5cPaperBrokerLifecycleError::EventTimestampBeforeAck)?;
        if position.account_id != *admission.account_id()
            || position_source_ts > position.received_ts
            || position.received_ts > evidence.truth.received_ts
        {
            return Err(Stage5cPaperBrokerLifecycleError::InstrumentMismatch);
        }
        if order.filled_qty > rust_decimal::Decimal::ZERO
            && (position_source_ts.timestamp() < ack.processed_ts_utc
                || position.received_ts.timestamp() < ack.processed_ts_utc)
        {
            return Err(Stage5cPaperBrokerLifecycleError::EventTimestampBeforeAck);
        }
        target_position_qty += position.qty;
        if position.qty != rust_decimal::Decimal::ZERO {
            let avg_price = position
                .avg_price
                .ok_or(Stage5cPaperBrokerLifecycleError::IntentFieldMismatch)?;
            target_position_value += avg_price * position.qty.abs();
            target_position_weight += position.qty.abs();
        }
        target_positions.push(position.clone());
    }
    let pre_position_qty = rust_decimal::Decimal::from_f64_retain(stage5cj_position_qty(
        Strategy::state(&resolved.strategy),
    ))
    .ok_or(Stage5cPaperBrokerLifecycleError::IntentFieldMismatch)?;
    let signed_fill = match expected_side {
        crate::BrokerNeutralOrderSide::Buy => order.filled_qty,
        crate::BrokerNeutralOrderSide::Sell => -order.filled_qty,
    };
    if target_position_qty != pre_position_qty + signed_fill {
        return Err(Stage5cPaperBrokerLifecycleError::PositionSideMismatch);
    }

    correlated_trades.sort_by(|left, right| {
        left.broker_trade_id
            .as_str()
            .cmp(right.broker_trade_id.as_str())
    });
    target_positions.sort_by(|left, right| {
        left.instrument
            .symbol
            .cmp(&right.instrument.symbol)
            .then_with(|| left.qty.cmp(&right.qty))
    });
    let target_avg_price = if target_position_weight > rust_decimal::Decimal::ZERO {
        target_position_value / target_position_weight
    } else if correlated_trade_qty > rust_decimal::Decimal::ZERO {
        correlated_trades
            .iter()
            .map(|trade| trade.price * trade.qty)
            .sum::<rust_decimal::Decimal>()
            / correlated_trade_qty
    } else {
        rust_decimal::Decimal::ZERO
    };
    let order_source_ts = order
        .source_ts
        .ok_or(Stage5cPaperBrokerLifecycleError::EventTimestampBeforeAck)?;
    let lifecycle_event_ts_utc = correlated_trades
        .iter()
        .map(|trade| trade.source_ts.timestamp())
        .chain(
            target_positions
                .iter()
                .filter_map(|position| position.source_ts.map(|value| value.timestamp())),
        )
        .fold(order_source_ts.timestamp(), i64::max);
    let evidence_fingerprint = stage5c_market_terminal_evidence_fingerprint(
        evidence.request_id,
        ack.status,
        broker_order_id,
        &expected_client_order_id,
        order,
        record.intent_class,
        expected_attribution,
        target_position_qty,
        lifecycle_event_ts_utc,
        evidence.truth.received_ts.timestamp(),
        &correlated_trades,
        &target_positions,
    );

    Ok(Stage5cValidatedMarketTerminalFacts {
        request_id: evidence.request_id,
        ack_status: ack.status,
        account_id: admission.account_id().clone(),
        broker_order_id: broker_order_id.clone(),
        client_order_id: expected_client_order_id,
        instrument: target.clone(),
        side: expected_side,
        attribution: expected_attribution.clone(),
        order_status: order.status.clone(),
        order_qty: order.qty,
        filled_qty: order.filled_qty,
        target_position_qty,
        target_avg_price,
        intent_class: record.intent_class,
        lifecycle_event_ts_utc,
        lifecycle_watermark_ts_utc: evidence.truth.received_ts.timestamp(),
        broker_event_count: 1 + correlated_trade_count + target_positions.len(),
        evidence_fingerprint,
        correlated_trades,
        target_positions,
    })
}

#[allow(dead_code)]
fn stage5c_order_side_matches(
    actual: broker_core::OrderSide,
    expected: crate::BrokerNeutralOrderSide,
) -> bool {
    matches!(
        (actual, expected),
        (
            broker_core::OrderSide::Buy,
            crate::BrokerNeutralOrderSide::Buy
        ) | (
            broker_core::OrderSide::Sell,
            crate::BrokerNeutralOrderSide::Sell
        )
    )
}

#[allow(dead_code)]
fn stage5c_market_terminal_runtime_ack(
    facts: &Stage5cValidatedMarketTerminalFacts,
) -> broker_core::HybridRuntimeCommandAck {
    let status = match facts.order_status {
        broker_core::OrderStatus::Rejected => broker_core::HybridRuntimeAckStatus::Rejected,
        broker_core::OrderStatus::Canceled | broker_core::OrderStatus::Expired => {
            broker_core::HybridRuntimeAckStatus::Expired
        }
        _ => broker_core::HybridRuntimeAckStatus::Error,
    };
    broker_core::HybridRuntimeCommandAck {
        request_id: facts.request_id,
        status,
        broker_order_id: Some(facts.broker_order_id.clone()),
        error_code: Some(broker_core::HybridRuntimeAckErrorCode::Other(format!(
            "market_terminal_{:?}",
            facts.order_status
        ))),
        error_message: Some("broker MARKET order reached terminal status".to_string()),
        processed_ts_utc: facts.lifecycle_event_ts_utc,
    }
}

#[allow(dead_code)]
fn stage5c_apply_market_terminal_position(
    strategy: &mut HybridIntradayRuntimeStrategy,
    admission: &Stage5cPaperHostAdmission,
    batch: &Stage5cPaperIntentBatch,
    qty: f64,
    avg_price: f64,
    source_ts_utc: i64,
    existing: bool,
) -> Result<Vec<crate::BrokerNeutralHybridIntent>, Stage5cPaperBrokerLifecycleFailure> {
    let context =
        stage5cj_broker_lifecycle_context(strategy, admission, batch.bar_close_ts(), source_ts_utc);
    crate::BrokerNeutralHybridStrategy::on_broker_position(
        strategy,
        broker_core::HybridRuntimeCallbackInput {
            context,
            payload: broker_core::HybridRuntimePositionEvent {
                instrument: admission.target_instrument().clone(),
                qty,
                existing,
                avg_price,
                source_ts_utc,
            },
        },
    )
    .map_err(|_| {
        Stage5cPaperBrokerLifecycleFailure::Terminal(
            Stage5cPaperBrokerLifecycleError::CallbackValidationFailed,
        )
    })
}

#[allow(dead_code)]
fn stage5c_market_terminal_state_is_coherent(
    state: &StrategyState,
    original_request_id: StrategyRequestId,
    intent_class: crate::BrokerNeutralHybridIntentClass,
    expected_position_qty: f64,
) -> bool {
    let StrategyState::HybridIntradayRuntime {
        last_position_qty,
        pending_entry_owner,
        pending_entry_request_id,
        pending_exit_request_id,
        ..
    } = state
    else {
        return false;
    };
    if !stage5cj_f64_eq(*last_position_qty, expected_position_qty)
        || *pending_entry_request_id == Some(original_request_id)
        || *pending_exit_request_id == Some(original_request_id)
    {
        return false;
    }
    match intent_class {
        crate::BrokerNeutralHybridIntentClass::Entry => pending_entry_owner.is_none(),
        crate::BrokerNeutralHybridIntentClass::Exit => true,
        crate::BrokerNeutralHybridIntentClass::ProtectiveRepair
        | crate::BrokerNeutralHybridIntentClass::CancelCleanup => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn stage5c_market_terminal_evidence_fingerprint(
    request_id: StrategyRequestId,
    ack_status: broker_core::HybridRuntimeAckStatus,
    broker_order_id: &BrokerOrderId,
    client_order_id: &broker_core::ClientOrderId,
    order: &broker_core::BrokerOrderSnapshot,
    intent_class: crate::BrokerNeutralHybridIntentClass,
    attribution: &broker_core::HybridRuntimeAttribution,
    target_position_qty: rust_decimal::Decimal,
    event_ts_utc: i64,
    received_ts_utc: i64,
    trades: &[broker_core::BrokerTradeSnapshot],
    positions: &[broker_core::BrokerPositionSnapshot],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(request_id.to_string());
    hasher.update(format!("{ack_status:?}|{intent_class:?}"));
    hasher.update(order.account_id.as_str());
    hasher.update(broker_order_id.as_str());
    hasher.update(client_order_id.as_str());
    hasher.update(&order.instrument.symbol);
    hasher.update(order.instrument.venue_symbol.as_deref().unwrap_or_default());
    hasher.update(format!(
        "{:?}|{:?}|{:?}",
        order.instrument.exchange, order.instrument.market, order.side
    ));
    hasher.update(format!("{:?}|{:?}", order.order_type, order.status));
    hasher.update(order.qty.to_string());
    hasher.update(order.filled_qty.to_string());
    hasher.update(
        order
            .remaining_qty
            .map(|value| value.to_string())
            .unwrap_or_default(),
    );
    hasher.update(
        order
            .source_ts
            .map(|value| value.timestamp())
            .unwrap_or_default()
            .to_be_bytes(),
    );
    hasher.update(order.received_ts.timestamp().to_be_bytes());
    hasher.update(attribution.internal_comment());
    hasher.update(target_position_qty.to_string());
    hasher.update(event_ts_utc.to_be_bytes());
    hasher.update(received_ts_utc.to_be_bytes());
    for trade in trades {
        hasher.update(trade.account_id.as_str());
        hasher.update(trade.broker_trade_id.as_str());
        hasher.update(
            trade
                .broker_order_id
                .as_ref()
                .map(BrokerOrderId::as_str)
                .unwrap_or_default(),
        );
        hasher.update(
            trade
                .client_order_id
                .as_ref()
                .map(broker_core::ClientOrderId::as_str)
                .unwrap_or_default(),
        );
        hasher.update(&trade.instrument.symbol);
        hasher.update(format!("{:?}", trade.side));
        hasher.update(trade.qty.to_string());
        hasher.update(trade.price.to_string());
        hasher.update(trade.source_ts.timestamp().to_be_bytes());
        hasher.update(trade.received_ts.timestamp().to_be_bytes());
    }
    for position in positions {
        hasher.update(position.account_id.as_str());
        hasher.update(&position.instrument.symbol);
        hasher.update(position.qty.to_string());
        hasher.update(
            position
                .avg_price
                .map(|value| value.to_string())
                .unwrap_or_default(),
        );
        hasher.update(
            position
                .source_ts
                .map(|value| value.timestamp())
                .unwrap_or_default()
                .to_be_bytes(),
        );
        hasher.update(position.received_ts.timestamp().to_be_bytes());
    }
    format!("{:x}", hasher.finalize())
}
// STAGE5G-C-R2CA-R1-AUTHORITY-END: market-terminal-state-coherence-v1

// STAGE5G-C-R2CA-R2-AUTHORITY-BEGIN: deterministic-terminal-fill-boundary-v1
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage5cMarketTerminalR2Error {
    SourceValidation(Stage5cPaperBrokerLifecycleError),
    FullFillStatusContradiction,
    SourceStateInconsistent,
    EvidenceTimeOverflow,
    EvidenceBeforeBracketTimer,
    CandidateAckFailed,
    CandidateAckGeneratedIntent,
    CandidatePositionFailed,
    CandidateIntentPolicyMismatch,
    CandidateStateIncoherent,
    CandidateEscrowFailed,
}

#[allow(dead_code)]
pub(crate) struct Stage5cMarketTerminalR2Blocked {
    reason: Stage5cMarketTerminalR2Error,
    resolved: Stage5cResolvedPaperIntentBatchStrategy,
}

#[allow(dead_code)]
impl Stage5cMarketTerminalR2Blocked {
    pub(crate) fn reason(&self) -> Stage5cMarketTerminalR2Error {
        self.reason
    }

    pub(crate) fn resolved(&self) -> &Stage5cResolvedPaperIntentBatchStrategy {
        &self.resolved
    }

    pub(crate) fn into_resolved(self) -> Stage5cResolvedPaperIntentBatchStrategy {
        self.resolved
    }
}

impl std::fmt::Debug for Stage5cMarketTerminalR2Blocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cMarketTerminalR2Blocked")
            .field("reason", &self.reason)
            .field("resolved_batch", &self.resolved.resolved_batch_summary())
            .finish_non_exhaustive()
    }
}

/// R2 validation-only capability.  It owns the exact R1 source capability,
/// source-owned owner/cycle projection and deterministic grace decision.
#[allow(dead_code)]
pub(crate) struct Stage5cValidatedMarketTerminalOutcomeR2 {
    validated_r1: Stage5cValidatedMarketTerminalOutcome,
    source_payload: crate::hybrid_intraday_runtime::Stage5gR2caR2SourcePayload,
    evidence_now_ms: i64,
    bracket_grace_active: bool,
}

#[allow(dead_code)]
fn stage5c_r2_block(
    reason: Stage5cMarketTerminalR2Error,
    resolved: Stage5cResolvedPaperIntentBatchStrategy,
) -> Box<Stage5cMarketTerminalR2Blocked> {
    Box::new(Stage5cMarketTerminalR2Blocked { reason, resolved })
}

/// Adds the frozen terminal status/fill matrix and full source-state preflight
/// before any strategy callback can run.
#[allow(dead_code)]
pub(crate) fn validate_stage5c_market_terminal_outcome_r2(
    resolved: Stage5cResolvedPaperIntentBatchStrategy,
    evidence: Stage5cMarketTerminalOrderEvidence,
) -> Result<Stage5cValidatedMarketTerminalOutcomeR2, Box<Stage5cMarketTerminalR2Blocked>> {
    let validated_r1 = match validate_stage5c_market_terminal_outcome(resolved, evidence) {
        Ok(validated) => validated,
        Err(Stage5cPaperBrokerLifecycleFailure::Blocked(blocked)) => {
            let reason = Stage5cMarketTerminalR2Error::SourceValidation(blocked.reason());
            return Err(stage5c_r2_block(reason, blocked.into_resolved()));
        }
        Err(Stage5cPaperBrokerLifecycleFailure::Terminal(reason)) => {
            panic!("R1 validation returned an impossible terminal failure: {reason:?}")
        }
    };
    let facts = &validated_r1.facts;
    if matches!(
        facts.order_status,
        broker_core::OrderStatus::Canceled | broker_core::OrderStatus::Expired
    ) && facts.filled_qty == facts.order_qty
    {
        return Err(stage5c_r2_block(
            Stage5cMarketTerminalR2Error::FullFillStatusContradiction,
            validated_r1.resolved,
        ));
    }
    let Some(source_payload) = validated_r1
        .resolved
        .strategy
        .stage5g_r2ca_r2_source_payload(facts.request_id, facts.intent_class, facts.side)
    else {
        return Err(stage5c_r2_block(
            Stage5cMarketTerminalR2Error::SourceStateInconsistent,
            validated_r1.resolved,
        ));
    };
    let Some(evidence_now_ms) = facts.lifecycle_event_ts_utc.checked_mul(1_000) else {
        return Err(stage5c_r2_block(
            Stage5cMarketTerminalR2Error::EvidenceTimeOverflow,
            validated_r1.resolved,
        ));
    };
    let bracket_started_ms = validated_r1
        .resolved
        .strategy
        .stage5g_r2ca_r2_bracket_reconcile_started_ms();
    if facts.intent_class == crate::BrokerNeutralHybridIntentClass::Exit
        && facts.filled_qty > rust_decimal::Decimal::ZERO
        && bracket_started_ms.is_some_and(|started| evidence_now_ms < started)
    {
        return Err(stage5c_r2_block(
            Stage5cMarketTerminalR2Error::EvidenceBeforeBracketTimer,
            validated_r1.resolved,
        ));
    }
    let bracket_grace_active = facts.intent_class == crate::BrokerNeutralHybridIntentClass::Exit
        && facts.filled_qty > rust_decimal::Decimal::ZERO
        && validated_r1
            .resolved
            .strategy
            .stage5g_r2ca_r2_bracket_reconcile_active_at(evidence_now_ms);
    Ok(Stage5cValidatedMarketTerminalOutcomeR2 {
        validated_r1,
        source_payload,
        evidence_now_ms,
        bracket_grace_active,
    })
}

/// Runs every callback and escrow check against an isolated candidate.  The
/// original resolved capability is committed only after all invariants pass;
/// any pre-commit failure returns it unchanged for corrected retry.
#[allow(dead_code)]
pub(crate) fn settle_stage5c_validated_market_terminal_outcome_r2(
    validated: Stage5cValidatedMarketTerminalOutcomeR2,
) -> Result<Stage5cBrokerLifecycleSettlement, Box<Stage5cMarketTerminalR2Blocked>> {
    let Stage5cValidatedMarketTerminalOutcomeR2 {
        validated_r1,
        source_payload,
        evidence_now_ms,
        bracket_grace_active,
    } = validated;
    let Stage5cValidatedMarketTerminalOutcome { resolved, facts } = validated_r1;
    let mut candidate = resolved.strategy.stage5g_r2ca_r2_transaction_candidate();
    let admission = &resolved
        .recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    let cleanup_ledger =
        stage5cj_cleanup_attribution_ledger(Strategy::state(&candidate), admission.strategy_id());
    let attempt = (|| {
        let position_qty = facts
            .target_position_qty
            .to_f64()
            .filter(|value| value.is_finite())
            .ok_or(Stage5cMarketTerminalR2Error::SourceStateInconsistent)?;
        let avg_price = facts
            .target_avg_price
            .to_f64()
            .filter(|value| value.is_finite())
            .ok_or(Stage5cMarketTerminalR2Error::SourceStateInconsistent)?;
        let is_positive_fill = facts.filled_qty > rust_decimal::Decimal::ZERO;
        let terminal_ack = stage5c_market_terminal_runtime_ack(&facts);
        let terminal_ack_intents =
            crate::BrokerNeutralHybridStrategy::on_broker_ack(&mut candidate, terminal_ack)
                .map_err(|_| Stage5cMarketTerminalR2Error::CandidateAckFailed)?;
        if !terminal_ack_intents.is_empty() {
            return Err(Stage5cMarketTerminalR2Error::CandidateAckGeneratedIntent);
        }

        let mut generated_intents = Vec::new();
        if is_positive_fill {
            let context = stage5cj_broker_lifecycle_context(
                &candidate,
                admission,
                resolved.resolved_batch.bar_close_ts(),
                facts.lifecycle_event_ts_utc,
            );
            let input = broker_core::HybridRuntimeCallbackInput {
                context,
                payload: broker_core::HybridRuntimePositionEvent {
                    instrument: admission.target_instrument().clone(),
                    qty: position_qty,
                    existing: false,
                    avg_price,
                    source_ts_utc: facts.lifecycle_event_ts_utc,
                },
            };
            let candidate_intents = match facts.intent_class {
                crate::BrokerNeutralHybridIntentClass::Entry => candidate
                    .stage5g_r2ca_r2_apply_partial_entry_position_at(input, source_payload)
                    .map_err(|_| Stage5cMarketTerminalR2Error::CandidatePositionFailed)?,
                crate::BrokerNeutralHybridIntentClass::Exit => candidate
                    .stage5g_r2ca_r2_apply_partial_exit_position_at(input, evidence_now_ms)
                    .map_err(|_| Stage5cMarketTerminalR2Error::CandidatePositionFailed)?,
                crate::BrokerNeutralHybridIntentClass::ProtectiveRepair
                | crate::BrokerNeutralHybridIntentClass::CancelCleanup => {
                    return Err(Stage5cMarketTerminalR2Error::SourceStateInconsistent)
                }
            };
            generated_intents.extend(candidate_intents);
            if generated_intents.is_empty() && !bracket_grace_active {
                return Err(Stage5cMarketTerminalR2Error::CandidateIntentPolicyMismatch);
            }
            if !generated_intents.is_empty() && bracket_grace_active {
                return Err(Stage5cMarketTerminalR2Error::CandidateIntentPolicyMismatch);
            }
        }

        if !stage5c_market_terminal_state_is_coherent(
            Strategy::state(&candidate),
            facts.request_id,
            facts.intent_class,
            position_qty,
        ) {
            return Err(Stage5cMarketTerminalR2Error::CandidateStateIncoherent);
        }
        if bracket_grace_active
            && candidate.stage5g_r2ca_r2_bracket_reconcile_started_ms()
                != resolved
                    .strategy
                    .stage5g_r2ca_r2_bracket_reconcile_started_ms()
        {
            return Err(Stage5cMarketTerminalR2Error::CandidateStateIncoherent);
        }

        let mut generated_intent_batch = None;
        let mut settled_batch_history = resolved.settled_batch_history.clone();
        if !generated_intents.is_empty() {
            let expected_attribution_by_request =
                stage5cj_expected_generated_attribution_by_request_from_ledger(
                    admission,
                    facts.lifecycle_event_ts_utc,
                    &generated_intents,
                    &cleanup_ledger,
                )
                .map_err(|_| Stage5cMarketTerminalR2Error::CandidateEscrowFailed)?;
            let mut callback_batch = stage5c_build_paper_intent_batch(
                &candidate,
                admission,
                facts.lifecycle_event_ts_utc,
                broker_core::HybridRuntimeBarOrigin::Live,
                generated_intents,
                &expected_attribution_by_request,
            )
            .map_err(|_| Stage5cMarketTerminalR2Error::CandidateEscrowFailed)?;
            stage5cj_verify_generated_batch_final_pending_consistency(
                Strategy::state(&candidate),
                &callback_batch,
            )
            .map_err(|_| Stage5cMarketTerminalR2Error::CandidateEscrowFailed)?;
            callback_batch.state_fingerprint =
                stage5c_state_fingerprint(Strategy::state(&candidate));
            settled_batch_history.push(stage5ch_batch_summary(&callback_batch));
            generated_intent_batch = Some(callback_batch);
        }
        Ok((
            candidate,
            generated_intent_batch,
            settled_batch_history,
            position_qty,
        ))
    })();

    let (strategy, generated_intent_batch, settled_batch_history, _) = match attempt {
        Ok(result) => result,
        Err(reason) => return Err(stage5c_r2_block(reason, resolved)),
    };
    let Stage5cResolvedPaperIntentBatchStrategy {
        recovery_receipt,
        resolved_batch,
        ack_outcomes,
        ..
    } = resolved;
    let resolved_batch_summary = stage5ch_batch_summary(&resolved_batch);
    let committed = Stage5cBrokerLifecycleResolvedPaperStrategy {
        strategy,
        recovery_receipt,
        resolved_batch,
        resolved_batch_summary,
        ack_outcomes,
        broker_event_count: facts.broker_event_count,
        remaining_lifecycle_expectations: Vec::new(),
        lifecycle_watermark_ts_utc: facts.lifecycle_watermark_ts_utc,
        generated_intent_batch,
        settled_batch_history,
    };
    Ok(settle_stage5c_broker_lifecycle_result(committed))
}
// STAGE5G-C-R2CA-R2-AUTHORITY-END: deterministic-terminal-fill-boundary-v1

// STAGE5G-C-R2CA-R3-AUTHORITY-BEGIN: exact-receipt-clock-bracket-authority-v1
/// R3 validation capability. The accepted R2 transaction settlement is kept
/// intact, while its bracket decision watermark is replaced with the exact
/// BrokerTruth package receipt clock that shares the timer's local clock
/// domain and millisecond precision.
#[allow(dead_code)]
pub(crate) struct Stage5cValidatedMarketTerminalOutcomeR3 {
    validated_r2: Stage5cValidatedMarketTerminalOutcomeR2,
    evidence_received_ms: i64,
}

#[allow(dead_code)]
impl Stage5cValidatedMarketTerminalOutcomeR3 {
    #[cfg(test)]
    fn evidence_received_ms(&self) -> i64 {
        self.evidence_received_ms
    }
}

/// Captures the exact package receipt timestamp before moving evidence into
/// the inherited R1 validator. Component source timestamps remain economic
/// identity/chronology only; they never decide bracket grace.
#[allow(dead_code)]
pub(crate) fn validate_stage5c_market_terminal_outcome_r3(
    resolved: Stage5cResolvedPaperIntentBatchStrategy,
    evidence: Stage5cMarketTerminalOrderEvidence,
) -> Result<Stage5cValidatedMarketTerminalOutcomeR3, Box<Stage5cMarketTerminalR2Blocked>> {
    let evidence_received_ms = evidence.truth.received_ts.timestamp_millis();
    let validated_r1 = match validate_stage5c_market_terminal_outcome(resolved, evidence) {
        Ok(validated) => validated,
        Err(Stage5cPaperBrokerLifecycleFailure::Blocked(blocked)) => {
            let reason = Stage5cMarketTerminalR2Error::SourceValidation(blocked.reason());
            return Err(stage5c_r2_block(reason, blocked.into_resolved()));
        }
        Err(Stage5cPaperBrokerLifecycleFailure::Terminal(reason)) => {
            panic!("R1 validation returned an impossible terminal failure: {reason:?}")
        }
    };
    let facts = &validated_r1.facts;
    if matches!(
        facts.order_status,
        broker_core::OrderStatus::Canceled | broker_core::OrderStatus::Expired
    ) && facts.filled_qty == facts.order_qty
    {
        return Err(stage5c_r2_block(
            Stage5cMarketTerminalR2Error::FullFillStatusContradiction,
            validated_r1.resolved,
        ));
    }
    let Some(source_payload) = validated_r1
        .resolved
        .strategy
        .stage5g_r2ca_r2_source_payload(facts.request_id, facts.intent_class, facts.side)
    else {
        return Err(stage5c_r2_block(
            Stage5cMarketTerminalR2Error::SourceStateInconsistent,
            validated_r1.resolved,
        ));
    };
    let Some(ack_processed_ms) = validated_r1
        .resolved
        .ack_outcomes
        .first()
        .and_then(|ack| ack.processed_ts_utc.checked_mul(1_000))
    else {
        return Err(stage5c_r2_block(
            Stage5cMarketTerminalR2Error::EvidenceTimeOverflow,
            validated_r1.resolved,
        ));
    };
    if evidence_received_ms < ack_processed_ms {
        return Err(stage5c_r2_block(
            Stage5cMarketTerminalR2Error::SourceValidation(
                Stage5cPaperBrokerLifecycleError::EventTimestampBeforeAck,
            ),
            validated_r1.resolved,
        ));
    }
    let bracket_started_ms = validated_r1
        .resolved
        .strategy
        .stage5g_r2ca_r2_bracket_reconcile_started_ms();
    let is_partial_exit = facts.intent_class == crate::BrokerNeutralHybridIntentClass::Exit
        && facts.filled_qty > rust_decimal::Decimal::ZERO;
    if is_partial_exit && bracket_started_ms.is_some_and(|started| evidence_received_ms < started) {
        return Err(stage5c_r2_block(
            Stage5cMarketTerminalR2Error::EvidenceBeforeBracketTimer,
            validated_r1.resolved,
        ));
    }
    let bracket_grace_active = is_partial_exit
        && validated_r1
            .resolved
            .strategy
            .stage5g_r2ca_r2_bracket_reconcile_active_at(evidence_received_ms);
    let validated_r2 = Stage5cValidatedMarketTerminalOutcomeR2 {
        validated_r1,
        source_payload,
        evidence_now_ms: evidence_received_ms,
        bracket_grace_active,
    };
    Ok(Stage5cValidatedMarketTerminalOutcomeR3 {
        validated_r2,
        evidence_received_ms,
    })
}

/// Delegates all mutation, rollback and escrow work to the pinned R2
/// transaction settlement after R3 has supplied a coherent receipt watermark.
#[allow(dead_code)]
pub(crate) fn settle_stage5c_validated_market_terminal_outcome_r3(
    validated: Stage5cValidatedMarketTerminalOutcomeR3,
) -> Result<Stage5cBrokerLifecycleSettlement, Box<Stage5cMarketTerminalR2Blocked>> {
    settle_stage5c_validated_market_terminal_outcome_r2(validated.validated_r2)
}
// STAGE5G-C-R2CA-R3-AUTHORITY-END: exact-receipt-clock-bracket-authority-v1

fn stage5cg_source_request_id(
    strategy_id: &str,
    account_id: &str,
    symbol: &str,
    bar_close_ts: i64,
    intent: &crate::BrokerNeutralHybridIntent,
) -> Result<StrategyRequestId, Stage5cIntentSettlementError> {
    use crate::BrokerNeutralHybridIntent as Intent;
    use crate::BrokerNeutralOrderSide as OrderSide;
    match intent.base_intent() {
        Intent::Place { .. } => Ok(crate::deterministic_request_id(
            strategy_id,
            account_id,
            symbol,
            "place",
            bar_close_ts,
            0,
        )),
        Intent::Cancel { order_id } => Ok(crate::deterministic_request_id(
            strategy_id,
            account_id,
            symbol,
            &format!("cancel:{}", order_id.as_str()),
            bar_close_ts,
            1,
        )),
        Intent::Replace { .. } => Ok(crate::deterministic_request_id(
            strategy_id,
            account_id,
            symbol,
            "replace",
            bar_close_ts,
            2,
        )),
        Intent::Market { side, .. } => {
            let seq = match side {
                OrderSide::Buy => 3,
                OrderSide::Sell => 4,
            };
            Ok(crate::deterministic_request_id(
                strategy_id,
                account_id,
                symbol,
                "market",
                bar_close_ts,
                seq,
            ))
        }
        Intent::CreateStopLimit { .. } => Ok(crate::deterministic_request_id(
            strategy_id,
            account_id,
            symbol,
            "create_stop_limit",
            bar_close_ts,
            5,
        )),
        Intent::DeleteStopLimit { order_id, .. } => Ok(crate::deterministic_request_id(
            strategy_id,
            account_id,
            symbol,
            &format!("delete_stop_limit:{}", order_id.as_str()),
            bar_close_ts,
            6,
        )),
        Intent::Classified { .. } | Intent::Routed { .. } => {
            Err(Stage5cIntentSettlementError::UnsupportedIntentAction)
        }
    }
}

fn stage5cg_verify_pending_request_id(
    state: &StrategyState,
    class: crate::BrokerNeutralHybridIntentClass,
    request_id: StrategyRequestId,
) -> Result<(), Stage5cIntentSettlementError> {
    let expected = match state {
        StrategyState::HybridIntradayRuntime {
            pending_entry_request_id,
            pending_exit_request_id,
            pending_tp_request_id,
            pending_sl_request_id,
            ..
        } => match class {
            crate::BrokerNeutralHybridIntentClass::Entry => *pending_entry_request_id,
            crate::BrokerNeutralHybridIntentClass::Exit => *pending_exit_request_id,
            crate::BrokerNeutralHybridIntentClass::ProtectiveRepair => {
                if *pending_tp_request_id == Some(request_id) {
                    *pending_tp_request_id
                } else {
                    *pending_sl_request_id
                }
            }
            crate::BrokerNeutralHybridIntentClass::CancelCleanup => {
                return Ok(());
            }
        },
        _ => None,
    };
    match expected {
        Some(expected) if expected == request_id => Ok(()),
        Some(_) => Err(Stage5cIntentSettlementError::RequestIdMismatch),
        None => Err(Stage5cIntentSettlementError::MissingPendingRequest),
    }
}

fn validate_stage5cg_intent(
    intent: &crate::BrokerNeutralHybridIntent,
    symbol: &str,
    tick_size: f64,
    bar_close_ts: i64,
) -> Result<(), Stage5cIntentSettlementError> {
    if intent.explicit_class().is_none() {
        return Err(Stage5cIntentSettlementError::MissingIntentClass);
    }
    if let crate::BrokerNeutralHybridIntent::Routed {
        intent,
        symbol: routed,
    } = intent
    {
        if routed != symbol {
            return Err(Stage5cIntentSettlementError::InstrumentNamespaceMismatch);
        }
        return validate_stage5cg_intent(intent, symbol, tick_size, bar_close_ts);
    }
    if let crate::BrokerNeutralHybridIntent::Classified { intent, .. } = intent {
        return validate_stage5cg_base_intent(intent, tick_size, bar_close_ts);
    }
    Err(Stage5cIntentSettlementError::MissingIntentClass)
}

fn validate_stage5cg_base_intent(
    intent: &crate::BrokerNeutralHybridIntent,
    tick_size: f64,
    bar_close_ts: i64,
) -> Result<(), Stage5cIntentSettlementError> {
    use crate::BrokerNeutralHybridIntent as Intent;
    let qty = match intent {
        Intent::Place { qty, price, .. } => {
            validate_stage5cg_price(*price, tick_size)?;
            Some(*qty)
        }
        Intent::Market {
            qty, fill_price, ..
        } => {
            if let Some(price) = fill_price {
                validate_stage5cg_price(*price, tick_size)?;
            }
            Some(*qty)
        }
        Intent::Replace {
            new_qty, new_price, ..
        } => {
            validate_stage5cg_price(*new_price, tick_size)?;
            Some(*new_qty)
        }
        Intent::CreateStopLimit {
            qty,
            trigger_price,
            price,
            stop_end_unix_time,
            ..
        } => {
            validate_stage5cg_price(*trigger_price, tick_size)?;
            validate_stage5cg_price(*price, tick_size)?;
            if *stop_end_unix_time <= bar_close_ts {
                return Err(Stage5cIntentSettlementError::InvalidStopEnd);
            }
            Some(*qty)
        }
        Intent::Cancel { .. } | Intent::DeleteStopLimit { .. } => None,
        Intent::Classified { .. } | Intent::Routed { .. } => {
            return validate_stage5cg_intent(intent, "", tick_size, bar_close_ts)
        }
    };
    if qty.is_some_and(|value| !value.is_finite() || value <= 0.0) {
        return Err(Stage5cIntentSettlementError::InvalidQuantity);
    }
    Ok(())
}

fn validate_stage5cg_price(price: f64, tick_size: f64) -> Result<(), Stage5cIntentSettlementError> {
    if !price.is_finite() || price <= 0.0 {
        return Err(Stage5cIntentSettlementError::InvalidPrice);
    }
    let ticks = price / tick_size;
    if (ticks - ticks.round()).abs() > 1e-9 {
        return Err(Stage5cIntentSettlementError::PriceNotTickAligned);
    }
    Ok(())
}

#[cfg(test)]
mod bootstrap_notification_tests {
    use super::*;
    use broker_core::{BrokerPositionSnapshot, BrokerStopOrderId, Exchange, Market};
    use chrono::TimeZone;
    use rust_decimal::Decimal;

    use crate::hybrid_intraday::{
        HybridOrchestratorConfig, IntradayBreakoutConfig, MeanReversionConfig,
    };
    use crate::hybrid_intraday_runtime::{
        HybridIntradayProfile, HybridIntradayRuntimeConfig, MeanReversionVariant, MrGatePolicy,
        RiskGateMode,
    };
    use crate::runtime_compat::MarketBuyAndCloseLiveOrderStyle;

    fn target() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn strategy(symbol: &str, tick_size: f64) -> HybridIntradayRuntimeStrategy {
        HybridIntradayRuntimeStrategy::new(HybridIntradayRuntimeConfig {
            symbol: symbol.to_string(),
            profile: HybridIntradayProfile::BaselineRuntimeHybrid,
            mr_variant: MeanReversionVariant::ClassicPrevDayRange,
            mr_gate_policy: MrGatePolicy::Disabled,
            risk_gate_mode: RiskGateMode::Disabled,
            risk_gate_seed_file: None,
            risk_gate_ledger_key: None,
            model_session_start_time: None,
            model_session_end_time: None,
            qty: 1.0,
            live_order_style: MarketBuyAndCloseLiveOrderStyle::Market,
            tick_size,
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
            pending_timeout_sec: 30,
            partial_entry_fill_timeout_ms: 3_000,
            mr_config: MeanReversionConfig::default(),
            breakout_config: IntradayBreakoutConfig::default(),
            orchestrator_config: HybridOrchestratorConfig::default(),
        })
    }

    fn admission(position_qty: Decimal, expires_at: DateTime<Utc>) -> Stage5cPaperHostAdmission {
        let checked_ts = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 0)
            .single()
            .expect("timestamp");
        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let target = target();
        let positions = if position_qty == Decimal::ZERO {
            Vec::new()
        } else {
            vec![BrokerPositionSnapshot {
                account_id: account_id.clone(),
                instrument: target.clone(),
                qty: position_qty,
                avg_price: Some(Decimal::new(222_750, 2)),
                unrealized_pnl: None,
                source_ts: Some(checked_ts),
                received_ts: checked_ts,
            }]
        };
        let bootstrap_snapshot = RuntimeHostBootstrapSnapshot {
            account_id: account_id.clone(),
            instrument: target.clone(),
            target_position_qty: position_qty,
            target_open_positions: positions,
            target_active_orders: Vec::new(),
            account_active_orders_count: 0,
            target_is_flat: position_qty == Decimal::ZERO,
            received_ts: checked_ts,
        };
        Stage5cPaperHostAdmission {
            schema_version: STAGE5C_PAPER_HOST_ADMISSION_SCHEMA_VERSION,
            checked_ts,
            issued_ts: checked_ts,
            expires_at,
            strategy_id: "hybrid_imoexf".to_string(),
            account_id,
            target_instrument: target,
            tick_size: 0.5,
            bootstrap_snapshot,
            paper_only: true,
            runtime_host_attached: false,
            intent_sink_attached: false,
        }
    }

    #[test]
    fn stage5cb_rechecks_expiry_before_notification_without_state_mutation() {
        let expiry = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 1, 0)
            .single()
            .expect("expiry");
        let strategy = strategy("IMOEXF", 0.5);
        let before = serde_json::to_value(Strategy::state(&strategy)).expect("state before");
        let admission = admission(Decimal::ONE, expiry);
        let result = validate_stage5cb_notification(
            &strategy,
            &admission,
            expiry + chrono::Duration::milliseconds(1),
        );
        assert!(matches!(
            result,
            Err(Stage5cBootstrapNotificationError::AdmissionExpired)
        ));
        let after = serde_json::to_value(Strategy::state(&strategy)).expect("state after");
        assert_eq!(before, after);
    }

    #[test]
    fn stage5cb_uses_exact_snapshot_and_opens_no_later_lifecycle_step() {
        let expiry = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 1, 0)
            .single()
            .expect("expiry");
        let exact_snapshot = admission(Decimal::ONE, expiry).bootstrap_snapshot().clone();
        let loaded = prepare_stage5c_without_runtime_state(
            strategy("IMOEXF", 0.5),
            admission(Decimal::ONE, expiry),
        );
        let bootstrapped = notify_stage5c_bootstrap_at(loaded, expiry)
            .expect("notification at expiry remains valid");
        let receipt = bootstrapped.receipt();

        assert_eq!(receipt.bootstrap_snapshot(), &exact_snapshot);
        assert_eq!(receipt.notified_ts(), expiry);
        assert!(!receipt.runtime_state_restored());
        assert!(!receipt.warmup_started());
        assert!(!receipt.pending_recovery_started());
        assert!(!receipt.semantic_bar_enabled());
        assert!(!receipt.intent_sink_attached());
        assert_eq!(receipt.strategy_id(), "hybrid_imoexf");
        let state = serde_json::to_value(Strategy::state(bootstrapped.strategy())).expect("state");
        assert_eq!(state["HybridIntradayRuntime"]["last_position_qty"], 1.0);
    }

    #[test]
    fn stage5cb_rejects_strategy_configured_for_another_symbol() {
        let expiry = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 1, 0)
            .single()
            .expect("expiry");
        let strategy = strategy("SBER", 0.5);
        assert_eq!(
            validate_stage5cb_notification(&strategy, &admission(Decimal::ZERO, expiry), expiry,),
            Err(Stage5cBootstrapNotificationError::StrategyTargetMismatch)
        );
    }

    #[test]
    fn stage5cb_rejects_strategy_tick_size_mismatch() {
        let expiry = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 1, 0)
            .single()
            .expect("expiry");
        let strategy = strategy("IMOEXF", 1.0);
        assert_eq!(
            validate_stage5cb_notification(&strategy, &admission(Decimal::ZERO, expiry), expiry,),
            Err(Stage5cBootstrapNotificationError::StrategyTickSizeMismatch)
        );
    }

    #[test]
    fn stage5cb_binding_error_does_not_mutate_strategy_state() {
        let expiry = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 1, 0)
            .single()
            .expect("expiry");
        let strategy = strategy("SBER", 1.0);
        let before = serde_json::to_value(Strategy::state(&strategy)).expect("state before");
        let _ = validate_stage5cb_notification(&strategy, &admission(Decimal::ONE, expiry), expiry);
        let after = serde_json::to_value(Strategy::state(&strategy)).expect("state after");
        assert_eq!(before, after);
    }

    fn restore_input(
        configured: &HybridIntradayRuntimeStrategy,
        accepted: &Stage5cPaperHostAdmission,
        persisted_ts: DateTime<Utc>,
    ) -> Stage5cRuntimeStateRestoreInput {
        let qty_decimal = accepted.bootstrap_snapshot().target_position_qty;
        let seed_loaded = prepare_stage5c_without_runtime_state(
            strategy("IMOEXF", 0.5),
            admission(qty_decimal, accepted.expires_at()),
        );
        let seeded = notify_stage5c_bootstrap_at(seed_loaded, persisted_ts).expect("seed state");
        let mut state = serde_json::to_value(Strategy::state(seeded.strategy()))
            .expect("persisted state value");
        let qty = accepted
            .bootstrap_snapshot()
            .target_position_qty
            .to_f64()
            .expect("qty");
        state["HybridIntradayRuntime"]["last_position_qty"] = serde_json::json!(qty);
        state["HybridIntradayRuntime"]["current_side"] = if qty > 0.0 {
            serde_json::json!("long")
        } else if qty < 0.0 {
            serde_json::json!("short")
        } else {
            serde_json::Value::Null
        };
        let (profile, mr_variant, mr_gate_policy, risk_gate_mode) =
            configured.stage5c_profile_binding();
        Stage5cRuntimeStateRestoreInput {
            schema_version: STAGE5C_RUNTIME_STATE_RESTORE_SCHEMA_VERSION,
            state_schema_version: 1,
            strategy_kind: "hybrid_intraday_runtime".to_string(),
            strategy_id: accepted.strategy_id().to_string(),
            account_id: accepted.account_id().clone(),
            instrument: accepted.target_instrument().clone(),
            tick_size: 0.5,
            config_fingerprint: configured.stage5c_config_fingerprint(),
            profile,
            mr_variant,
            mr_gate_policy,
            risk_gate_mode,
            persisted_ts,
            state_json: serde_json::to_string(&state).expect("state JSON"),
            known_order_ids: Vec::new(),
            pending_requests: Vec::new(),
            legacy_numeric_order_id_policy: Stage5cLegacyNumericOrderIdPolicy::Reject,
        }
    }

    #[test]
    fn stage5cc_restores_same_strategy_and_opens_no_later_gate() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .expect("timestamp");
        let strategy = strategy("IMOEXF", 0.5);
        let admission = admission(Decimal::ONE, now + chrono::Duration::minutes(1));
        let input = restore_input(&strategy, &admission, now);
        let loaded = restore_stage5c_runtime_state_at(strategy, admission, input, now)
            .expect("validated load");
        let bootstrapped = notify_stage5c_bootstrap_at(loaded, now).expect("bootstrap");
        let restored = notify_stage5c_runtime_state_restored_at(bootstrapped, now)
            .expect("restore notification");

        assert!(restored.receipt().runtime_state_restored());
        assert!(!restored.receipt().warmup_started());
        assert!(!restored.receipt().pending_recovery_started());
        assert!(!restored.receipt().semantic_bar_enabled());
        assert!(!restored.receipt().intent_sink_attached());
        let state = serde_json::to_value(Strategy::state(restored.strategy())).expect("state");
        assert_eq!(state["HybridIntradayRuntime"]["last_position_qty"], 1.0);
    }

    #[test]
    fn stage5cc_rejects_state_that_overrides_broker_truth_position() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .expect("timestamp");
        let strategy = strategy("IMOEXF", 0.5);
        let admission = admission(Decimal::ONE, now + chrono::Duration::minutes(1));
        let mut input = restore_input(&strategy, &admission, now);
        let mut state: serde_json::Value =
            serde_json::from_str(&input.state_json).expect("state value");
        state["HybridIntradayRuntime"]["last_position_qty"] = serde_json::json!(0.0);
        input.state_json = serde_json::to_string(&state).expect("state JSON");

        assert!(matches!(
            restore_stage5c_runtime_state_at(strategy, admission, input, now),
            Err(Stage5cRuntimeStateRestoreError::BrokerTruthPositionMismatch)
        ));
    }

    #[test]
    fn stage5cc_requires_explicit_legacy_numeric_order_id_policy() {
        let mut numeric = serde_json::json!({"tp_order_id": 123});
        normalize_legacy_order_ids(
            &mut numeric,
            Stage5cLegacyNumericOrderIdPolicy::ConvertPositiveAlorNumeric,
        )
        .expect("positive conversion");
        assert_eq!(numeric["tp_order_id"], "123");
    }

    #[test]
    fn stage5cc_rejects_short_side_for_long_broker_position() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let strategy = strategy("IMOEXF", 0.5);
        let admission = admission(Decimal::ONE, now + chrono::Duration::minutes(1));
        let mut input = restore_input(&strategy, &admission, now);
        let mut state: serde_json::Value = serde_json::from_str(&input.state_json).unwrap();
        state["HybridIntradayRuntime"]["current_side"] = serde_json::json!("short");
        input.state_json = serde_json::to_string(&state).unwrap();
        assert!(matches!(
            restore_stage5c_runtime_state_at(strategy, admission, input, now),
            Err(Stage5cRuntimeStateRestoreError::BrokerTruthSideMismatch)
        ));
    }

    #[test]
    fn stage5cc_rejects_long_side_for_short_broker_position() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let strategy = strategy("IMOEXF", 0.5);
        let admission = admission(Decimal::NEGATIVE_ONE, now + chrono::Duration::minutes(1));
        let mut input = restore_input(&strategy, &admission, now);
        let mut state: serde_json::Value = serde_json::from_str(&input.state_json).unwrap();
        state["HybridIntradayRuntime"]["current_side"] = serde_json::json!("long");
        input.state_json = serde_json::to_string(&state).unwrap();
        assert!(matches!(
            restore_stage5c_runtime_state_at(strategy, admission, input, now),
            Err(Stage5cRuntimeStateRestoreError::BrokerTruthSideMismatch)
        ));
    }

    #[test]
    fn stage5cc_bootstrap_removes_stale_persisted_broker_ids() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let strategy = strategy("IMOEXF", 0.5);
        let admission = admission(Decimal::ONE, now + chrono::Duration::minutes(1));
        let mut input = restore_input(&strategy, &admission, now);
        let mut state: serde_json::Value = serde_json::from_str(&input.state_json).unwrap();
        state["HybridIntradayRuntime"]["tp_order_id"] = serde_json::json!("123");
        state["HybridIntradayRuntime"]["sl_stop_order_id"] = serde_json::json!("STOP-OLD");
        state["HybridIntradayRuntime"]["sl_exchange_order_id"] = serde_json::json!("456");
        input.state_json = serde_json::to_string(&state).unwrap();
        let loaded = restore_stage5c_runtime_state_at(strategy, admission, input, now).unwrap();
        let bootstrapped = notify_stage5c_bootstrap_at(loaded, now).unwrap();
        let restored = notify_stage5c_runtime_state_restored_at(bootstrapped, now).unwrap();
        let state = serde_json::to_value(Strategy::state(restored.strategy())).unwrap();
        assert!(state["HybridIntradayRuntime"]["tp_order_id"].is_null());
        assert!(state["HybridIntradayRuntime"]["sl_stop_order_id"].is_null());
        assert!(state["HybridIntradayRuntime"]["sl_exchange_order_id"].is_null());
    }

    #[test]
    fn stage5cc_legacy_numeric_conversion_and_invalid_matrix() {
        for invalid in [
            serde_json::json!(0),
            serde_json::json!(-1),
            serde_json::json!(1.5),
            serde_json::json!(u64::MAX),
        ] {
            let mut value = serde_json::json!({"tp_order_id": invalid});
            assert_eq!(
                normalize_legacy_order_ids(
                    &mut value,
                    Stage5cLegacyNumericOrderIdPolicy::ConvertPositiveAlorNumeric,
                ),
                Err(Stage5cRuntimeStateRestoreError::InvalidLegacyNumericOrderId)
            );
        }
        let mut rejected = serde_json::json!({"sl_exchange_order_id": 456});
        assert_eq!(
            normalize_legacy_order_ids(&mut rejected, Stage5cLegacyNumericOrderIdPolicy::Reject),
            Err(Stage5cRuntimeStateRestoreError::LegacyNumericOrderIdRejected)
        );
        let mut string_ids = serde_json::json!({"tp_order_id": "FINAM-123"});
        normalize_legacy_order_ids(
            &mut string_ids,
            Stage5cLegacyNumericOrderIdPolicy::ConvertPositiveAlorNumeric,
        )
        .unwrap();
        assert_eq!(string_ids["tp_order_id"], "FINAM-123");
    }

    #[test]
    fn stage5cc_full_state_converts_positive_numeric_tp_and_sl_ids() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let strategy = strategy("IMOEXF", 0.5);
        let admission = admission(Decimal::ONE, now + chrono::Duration::minutes(1));
        let mut input = restore_input(&strategy, &admission, now);
        let mut state: serde_json::Value = serde_json::from_str(&input.state_json).unwrap();
        state["HybridIntradayRuntime"]["tp_order_id"] = serde_json::json!(123);
        state["HybridIntradayRuntime"]["sl_exchange_order_id"] = serde_json::json!(456);
        input.state_json = serde_json::to_string(&state).unwrap();
        input.legacy_numeric_order_id_policy =
            Stage5cLegacyNumericOrderIdPolicy::ConvertPositiveAlorNumeric;

        let loaded = restore_stage5c_runtime_state_at(strategy, admission, input, now).unwrap();
        let state = serde_json::to_value(Strategy::state(&loaded.strategy)).unwrap();
        assert_eq!(state["HybridIntradayRuntime"]["tp_order_id"], "123");
        assert_eq!(
            state["HybridIntradayRuntime"]["sl_exchange_order_id"],
            "456"
        );
    }

    fn restored_strategy(now: DateTime<Utc>) -> Stage5cRuntimeStateRestoredPaperStrategy {
        let strategy = strategy("IMOEXF", 0.5);
        let admission = admission(Decimal::ZERO, now + chrono::Duration::minutes(2));
        let input = restore_input(&strategy, &admission, now);
        let loaded = restore_stage5c_runtime_state_at(strategy, admission, input, now).unwrap();
        let bootstrapped = notify_stage5c_bootstrap_at(loaded, now).unwrap();
        notify_stage5c_runtime_state_restored_at(bootstrapped, now).unwrap()
    }

    fn history_bar(close_time_utc: i64) -> broker_core::HybridRuntimeBarEvent {
        broker_core::HybridRuntimeBarEvent {
            instrument: target(),
            close_time_utc,
            open: 2200.0,
            high: 2202.0,
            low: 2199.0,
            close: 2201.0,
            volume: 100.0,
            origin: broker_core::HybridRuntimeBarOrigin::History,
            is_final: true,
            timeframe_sec: 600,
        }
    }

    fn accepted_history(
        bars: Vec<broker_core::HybridRuntimeBarEvent>,
    ) -> Stage5cAcceptedHistoryBatch {
        accept_stage5c_history_batch(Stage5cHistoryBatchInput {
            bars,
            provenance: broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(
            ),
        })
        .expect("canonical history")
    }

    fn warmed_strategy(now: DateTime<Utc>) -> Stage5cWarmedPaperStrategy {
        let close_ts = Utc
            .with_ymd_and_hms(2026, 7, 10, 10, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        warmup_stage5c_history_at(
            restored_strategy(now),
            accepted_history(vec![history_bar(close_ts)]),
            now,
        )
        .unwrap()
    }

    #[test]
    fn stage5cd_warms_canonical_history_without_opening_later_gates() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let close_ts = Utc
            .with_ymd_and_hms(2026, 7, 10, 10, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        let warmed = warmup_stage5c_history_at(
            restored_strategy(now),
            accepted_history(vec![history_bar(close_ts)]),
            now,
        )
        .unwrap();
        assert!(warmed.receipt().warmup_started());
        assert_eq!(warmed.receipt().processed_bars(), 1);
        assert!(!warmed.receipt().pending_recovery_started());
        assert!(!warmed.receipt().semantic_bar_enabled());
        assert!(!warmed.receipt().intent_sink_attached());
        let _ = Strategy::state(warmed.strategy());
    }

    #[test]
    fn stage5cd_rejects_noncanonical_history_matrix() {
        let close_ts = Utc
            .with_ymd_and_hms(2026, 7, 10, 10, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        let mut wrong_timeframe = history_bar(close_ts);
        wrong_timeframe.timeframe_sec = 60;
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![wrong_timeframe],
                provenance:
                    broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            }),
            Err(Stage5cHistoryWarmupError::InvalidTimeframe)
        ));
        let duplicate = history_bar(close_ts);
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![duplicate.clone(), duplicate],
                provenance:
                    broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            }),
            Err(Stage5cHistoryWarmupError::NonMonotonicTimestamp)
        ));
        let mut forming = history_bar(close_ts);
        forming.is_final = false;
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![forming],
                provenance:
                    broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            }),
            Err(Stage5cHistoryWarmupError::NonFinalBar)
        ));

        let mut wrong_origin = history_bar(close_ts);
        wrong_origin.origin = broker_core::HybridRuntimeBarOrigin::Live;
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![wrong_origin],
                provenance:
                    broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            }),
            Err(Stage5cHistoryWarmupError::InvalidOrigin)
        ));
        let mut invalid_ohlc = history_bar(close_ts);
        invalid_ohlc.high = 2190.0;
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![invalid_ohlc],
                provenance:
                    broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            }),
            Err(Stage5cHistoryWarmupError::InvalidOhlc)
        ));
    }

    #[test]
    fn stage5cd_rechecks_freshness_and_lifecycle_clock() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let close_ts = Utc
            .with_ymd_and_hms(2026, 7, 10, 10, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert!(matches!(
            warmup_stage5c_history_at(
                restored_strategy(now),
                accepted_history(vec![history_bar(close_ts)]),
                now + chrono::Duration::minutes(3),
            ),
            Err(Stage5cHistoryWarmupError::BrokerTruthExpired)
        ));
        assert!(matches!(
            warmup_stage5c_history_at(
                restored_strategy(now),
                accepted_history(vec![history_bar(close_ts)]),
                now - chrono::Duration::seconds(1),
            ),
            Err(Stage5cHistoryWarmupError::LifecycleTimestampReversal)
        ));
    }

    #[test]
    fn stage5cd_rejects_unapproved_stage3_provenance_matrix() {
        let close_ts = Utc
            .with_ymd_and_hms(2026, 7, 10, 10, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        for provenance in [
            broker_core::Stage3StrategyBarProvenance::raw_finam_m1(),
            broker_core::Stage3StrategyBarProvenance::finam_native_m10_pending(),
        ] {
            assert!(matches!(
                accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                    bars: vec![history_bar(close_ts)],
                    provenance,
                }),
                Err(Stage5cHistoryWarmupError::Stage3ProvenanceRejected)
            ));
        }
        let mut incomplete =
            broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete();
        incomplete.aggregation_complete = false;
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![history_bar(close_ts)],
                provenance: incomplete,
            }),
            Err(Stage5cHistoryWarmupError::Stage3ProvenanceRejected)
        ));
        let mut gap_unproven =
            broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete();
        gap_unproven.gap_absence_proven = false;
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![history_bar(close_ts)],
                provenance: gap_unproven,
            }),
            Err(Stage5cHistoryWarmupError::Stage3ProvenanceRejected)
        ));
    }

    #[test]
    fn stage5cd_rejects_future_and_unrepresentable_history_timestamps() {
        let now = Utc.with_ymd_and_hms(2026, 7, 13, 9, 0, 0).single().unwrap();
        let future = now.timestamp() + 600;
        assert!(matches!(
            warmup_stage5c_history_at(
                restored_strategy(now),
                accepted_history(vec![history_bar(future)]),
                now,
            ),
            Err(Stage5cHistoryWarmupError::FutureHistoryBar)
        ));
        let unrepresentable = i64::MAX - i64::MAX.rem_euclid(600);
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![history_bar(unrepresentable)],
                provenance:
                    broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            }),
            Err(Stage5cHistoryWarmupError::InvalidHistoryTimestamp)
        ));
        assert!(warmup_stage5c_history_at(
            restored_strategy(now),
            accepted_history(vec![history_bar(now.timestamp())]),
            now,
        )
        .is_ok());
    }

    #[test]
    fn stage5cd_executes_remaining_history_error_matrix() {
        let close_ts = Utc
            .with_ymd_and_hms(2026, 7, 10, 10, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: Vec::new(),
                provenance:
                    broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            }),
            Err(Stage5cHistoryWarmupError::EmptyHistory)
        ));
        let mut another_instrument = history_bar(close_ts + 600);
        another_instrument.instrument.symbol = "RI".to_string();
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![history_bar(close_ts), another_instrument],
                provenance:
                    broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            }),
            Err(Stage5cHistoryWarmupError::InstrumentMismatch)
        ));
        let mut unaligned = history_bar(close_ts + 1);
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![unaligned.clone()],
                provenance:
                    broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            }),
            Err(Stage5cHistoryWarmupError::UnalignedTimestamp)
        ));
        unaligned.close_time_utc = close_ts;
        unaligned.volume = -1.0;
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![unaligned],
                provenance:
                    broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            }),
            Err(Stage5cHistoryWarmupError::InvalidVolume)
        ));
        let saturday = Utc.with_ymd_and_hms(2026, 7, 11, 9, 0, 0).single().unwrap();
        assert!(matches!(
            warmup_stage5c_history_at(
                restored_strategy(saturday),
                accepted_history(vec![history_bar(saturday.timestamp())]),
                saturday,
            ),
            Err(Stage5cHistoryWarmupError::NoEligibleHistoryBars)
        ));
    }

    #[test]
    fn stage5cd_rejected_provenance_and_future_time_do_not_mutate_strategy() {
        let strategy = strategy("IMOEXF", 0.5);
        let before = serde_json::to_value(Strategy::state(&strategy)).unwrap();
        let close_ts = Utc
            .with_ymd_and_hms(2026, 7, 10, 10, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert!(accept_stage5c_history_batch(Stage5cHistoryBatchInput {
            bars: vec![history_bar(close_ts)],
            provenance: broker_core::Stage3StrategyBarProvenance::raw_finam_m1(),
        })
        .is_err());
        assert_eq!(
            before,
            serde_json::to_value(Strategy::state(&strategy)).unwrap()
        );

        let now = Utc.with_ymd_and_hms(2026, 7, 13, 9, 0, 0).single().unwrap();
        let restored = restored_strategy(now);
        let before = serde_json::to_value(Strategy::state(restored.strategy())).unwrap();
        let history = accepted_history(vec![history_bar(now.timestamp() + 600)]);
        let admission = &restored.receipt().bootstrap_receipt().admission;
        assert_eq!(
            validate_stage5cd_time_boundary(&history, admission, now),
            Err(Stage5cHistoryWarmupError::FutureHistoryBar)
        );
        assert_eq!(
            before,
            serde_json::to_value(Strategy::state(restored.strategy())).unwrap()
        );
    }

    #[test]
    fn stage5ce_recovers_complete_empty_pending_set_without_opening_later_gates() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let warmed = warmed_strategy(now);
        let proof = recovery_claim(&warmed, now).unwrap();
        let evidence =
            accept_stage5c_pending_recovery_evidence(Stage5cPendingRecoveryEvidenceInput {
                events: Vec::new(),
                claim_proof: proof,
            })
            .unwrap();
        let recovered =
            recover_stage5c_pending_streams_at(warmed, evidence, now).expect("empty recovery");
        assert!(recovered.receipt().pending_recovery_started());
        assert_eq!(recovered.receipt().replayed_events(), 0);
        assert!(!recovered.receipt().semantic_bar_enabled());
        assert!(!recovered.receipt().intent_sink_attached());
        let _ = Strategy::state(recovered.strategy());
    }

    #[test]
    fn stage5ce_deduplicates_identical_pending_events() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let event = Stage5cPendingRecoveryEvent {
            stream_kind: Stage5cPendingStreamKind::Position,
            stream_name: "broker.positions.ACC_TEST_0001".to_string(),
            entry_id: "1-0".to_string(),
            sequence: 1,
            payload: Stage5cPendingRecoveryPayload::Position(
                broker_core::HybridRuntimePositionEvent {
                    instrument: target(),
                    qty: 0.0,
                    existing: true,
                    avg_price: 0.0,
                    source_ts_utc: now.timestamp(),
                },
            ),
        };
        let warmed = warmed_strategy(now);
        let mut proof = recovery_claim(&warmed, now).unwrap();
        proof
            .streams
            .iter_mut()
            .find(|stream| stream.stream_kind == Stage5cPendingStreamKind::Position)
            .unwrap()
            .claimed_count = 1;
        let evidence =
            accept_stage5c_pending_recovery_evidence(Stage5cPendingRecoveryEvidenceInput {
                events: vec![event.clone(), event],
                claim_proof: proof,
            })
            .unwrap();
        let recovered = recover_stage5c_pending_streams_at(warmed, evidence, now)
            .expect("deduplicated recovery");
        assert_eq!(recovered.receipt().replayed_events(), 1);
        assert_eq!(recovered.receipt().duplicate_events(), 1);
    }

    #[test]
    fn stage5ce_rejects_incomplete_and_conflicting_recovery_evidence() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let warmed = warmed_strategy(now);
        assert!(matches!(
            recovery_claim_with_cursor(&warmed, now, "1-0"),
            Err(Stage5cPendingRecoveryError::ClaimBoundaryInvalid)
        ));
    }

    fn recovery_claim(
        warmed: &Stage5cWarmedPaperStrategy,
        now: DateTime<Utc>,
    ) -> Result<Stage5cPendingRecoveryClaimProof, Stage5cPendingRecoveryError> {
        recovery_claim_with_cursor(warmed, now, "0-0")
    }

    fn recovery_claim_with_cursor(
        warmed: &Stage5cWarmedPaperStrategy,
        now: DateTime<Utc>,
        cursor: &str,
    ) -> Result<Stage5cPendingRecoveryClaimProof, Stage5cPendingRecoveryError> {
        let admission = &warmed
            .receipt()
            .restore_receipt()
            .bootstrap_receipt()
            .admission;
        let names = [
            (Stage5cPendingStreamKind::Ack, "cmd.acks.ACC_TEST_0001"),
            (
                Stage5cPendingStreamKind::Order,
                "broker.orders.ACC_TEST_0001",
            ),
            (
                Stage5cPendingStreamKind::StopOrder,
                "broker.stop_orders.ACC_TEST_0001",
            ),
            (
                Stage5cPendingStreamKind::Position,
                "broker.positions.ACC_TEST_0001",
            ),
        ];
        prove_stage5c_pending_recovery_claim(
            warmed,
            Stage5cPendingRecoveryClaimProofInput {
                strategy_id: admission.strategy_id().to_string(),
                account_id: admission.account_id().clone(),
                target_instrument: admission.target_instrument().clone(),
                snapshot_received_ts: admission.bootstrap_snapshot().received_ts,
                completed_ts: now,
                streams: names
                    .into_iter()
                    .map(
                        |(stream_kind, stream_name)| Stage5cPendingStreamClaimBoundary {
                            stream_kind,
                            stream_name: stream_name.to_string(),
                            consumer_group: "paper-runtime:ACC_TEST_0001:hybrid_imoexf".to_string(),
                            terminal_claim_cursor: cursor.to_string(),
                            snapshot_boundary_entry_id: "0-0".to_string(),
                            claimed_count: 0,
                        },
                    )
                    .collect(),
            },
        )
    }

    fn semantic_input(close_ts: i64) -> Stage5cSemanticBarInput {
        let mut bar = history_bar(close_ts);
        bar.origin = broker_core::HybridRuntimeBarOrigin::Live;
        Stage5cSemanticBarInput {
            bar,
            provenance: broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(
            ),
            tick_size: 0.5,
        }
    }

    #[test]
    fn stage5cf_validates_actual_payload_matrix() {
        let close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 10, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert!(accept_stage5c_semantic_bar(semantic_input(close_ts)).is_ok());
        assert!(matches!(
            accept_stage5c_semantic_bar(semantic_input(close_ts + 1)),
            Err(Stage5cSemanticBarError::UnalignedTimestamp)
        ));
        let mut invalid = semantic_input(close_ts);
        invalid.bar.high = 2190.0;
        assert!(matches!(
            accept_stage5c_semantic_bar(invalid),
            Err(Stage5cSemanticBarError::InvalidOhlc)
        ));
        let mut non_finite = semantic_input(close_ts);
        non_finite.bar.close = f64::NAN;
        assert!(matches!(
            accept_stage5c_semantic_bar(non_finite),
            Err(Stage5cSemanticBarError::InvalidOhlc)
        ));
        let mut volume = semantic_input(close_ts);
        volume.bar.volume = -1.0;
        assert!(matches!(
            accept_stage5c_semantic_bar(volume),
            Err(Stage5cSemanticBarError::InvalidVolume)
        ));
    }

    #[test]
    fn stage5cf_context_uses_current_strategy_position() {
        let now = Utc.with_ymd_and_hms(2026, 7, 13, 9, 0, 0).single().unwrap();
        let admission = admission(Decimal::ZERO, now + chrono::Duration::minutes(20));
        let mut strategy = strategy("IMOEXF", 0.5);
        let context = StrategyCtx {
            strategy_id: "hybrid_imoexf".to_string(),
            portfolio: "ACC_TEST_0001".to_string(),
            exchange: "Moex".to_string(),
            symbol: "IMOEXF".to_string(),
            tick_size: 0.5,
            trade_mode: TradeMode::Paper,
            paper_execution_mode: PaperExecutionMode::LiveOnly,
            allow_live_orders: false,
            gateway_phase: GatewayPhase::SyncingGap,
            position_qty: Some(1.0),
            event_ts_utc: now.timestamp(),
            now_ts_utc: now.timestamp(),
            last_bar_ts: None,
        };
        Strategy::on_position(
            &mut strategy,
            &context,
            &PositionEvent {
                symbol: "IMOEXF".to_string(),
                qty: 1.0,
                existing: true,
                avg_price: 2200.0,
                ts_utc: now.timestamp(),
            },
        );
        let semantic = stage5cf_semantic_context(&strategy, &admission, now.timestamp(), now);
        assert_eq!(semantic.position_qty, Some(1.0));
    }

    fn empty_recovered(now: DateTime<Utc>) -> Stage5cPendingRecoveredPaperStrategy {
        let warmed = warmed_strategy(now);
        let proof = recovery_claim(&warmed, now).unwrap();
        let evidence =
            accept_stage5c_pending_recovery_evidence(Stage5cPendingRecoveryEvidenceInput {
                events: Vec::new(),
                claim_proof: proof,
            })
            .unwrap();
        recover_stage5c_pending_streams_at(warmed, evidence, now).unwrap()
    }

    fn empty_recovered_until(
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Stage5cPendingRecoveredPaperStrategy {
        let strategy = strategy("IMOEXF", 0.5);
        let admission = admission(Decimal::ZERO, expires_at);
        let input = restore_input(&strategy, &admission, now);
        let loaded = restore_stage5c_runtime_state_at(strategy, admission, input, now).unwrap();
        let bootstrapped = notify_stage5c_bootstrap_at(loaded, now).unwrap();
        let restored = notify_stage5c_runtime_state_restored_at(bootstrapped, now).unwrap();
        let warmed = warmup_stage5c_history_at(
            restored,
            accepted_history(vec![history_bar(
                Utc.with_ymd_and_hms(2026, 7, 10, 10, 0, 0)
                    .single()
                    .unwrap()
                    .timestamp(),
            )]),
            now,
        )
        .unwrap();
        let proof = recovery_claim(&warmed, now).unwrap();
        let evidence =
            accept_stage5c_pending_recovery_evidence(Stage5cPendingRecoveryEvidenceInput {
                events: Vec::new(),
                claim_proof: proof,
            })
            .unwrap();
        recover_stage5c_pending_streams_at(warmed, evidence, now).unwrap()
    }

    fn set_hybrid_pending_request(
        strategy: &mut HybridIntradayRuntimeStrategy,
        class: crate::BrokerNeutralHybridIntentClass,
        request_id: StrategyRequestId,
    ) {
        let mut state = Strategy::state(strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                pending_entry_request_id,
                pending_exit_request_id,
                pending_tp_request_id,
                ..
            } => match class {
                crate::BrokerNeutralHybridIntentClass::Entry => {
                    *pending_entry_request_id = Some(request_id);
                }
                crate::BrokerNeutralHybridIntentClass::Exit => {
                    *pending_exit_request_id = Some(request_id);
                }
                crate::BrokerNeutralHybridIntentClass::ProtectiveRepair => {
                    *pending_tp_request_id = Some(request_id);
                }
                crate::BrokerNeutralHybridIntentClass::CancelCleanup => {}
            },
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(strategy, state);
    }

    fn set_hybrid_pending_sl_request(
        strategy: &mut HybridIntradayRuntimeStrategy,
        request_id: StrategyRequestId,
    ) {
        let mut state = Strategy::state(strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                pending_sl_request_id,
                ..
            } => {
                *pending_sl_request_id = Some(request_id);
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(strategy, state);
    }

    fn stage5cg_semantic_result(
        strategy: HybridIntradayRuntimeStrategy,
        recovery_receipt: Stage5cPendingRecoveryReceipt,
        bar_close_ts: i64,
        origin: broker_core::HybridRuntimeBarOrigin,
        intents: Vec<crate::BrokerNeutralHybridIntent>,
    ) -> Stage5cSemanticBarResult {
        Stage5cSemanticBarResult {
            strategy,
            recovery_receipt,
            bar_close_ts,
            origin,
            execution_eligible: origin == broker_core::HybridRuntimeBarOrigin::Live,
            intents,
            expected_attribution_by_request: HashMap::new(),
        }
    }

    fn stage5cg_market_intent(
        side: crate::BrokerNeutralOrderSide,
        class: crate::BrokerNeutralHybridIntentClass,
    ) -> crate::BrokerNeutralHybridIntent {
        crate::BrokerNeutralHybridIntent::Market {
            qty: 1.0,
            side,
            fill_price: Some(2227.5),
            comment: None,
        }
        .with_class(class)
        .with_symbol("IMOEXF")
    }

    fn stage5cg_place_intent() -> crate::BrokerNeutralHybridIntent {
        crate::BrokerNeutralHybridIntent::Place {
            price: 2230.0,
            qty: 1.0,
            side: crate::BrokerNeutralOrderSide::Sell,
            comment: Some("HYB|sid=hybrid_imoexf|c=abc1230001|o=MR|r=TP".to_string()),
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::ProtectiveRepair)
        .with_symbol("IMOEXF")
    }

    fn stage5cg_stop_intent(stop_end_unix_time: i64) -> crate::BrokerNeutralHybridIntent {
        crate::BrokerNeutralHybridIntent::CreateStopLimit {
            side: crate::BrokerNeutralOrderSide::Sell,
            qty: 1.0,
            trigger_price: 2210.0,
            price: 2209.5,
            condition: crate::runtime_compat::StopLimitCondition::LessOrEqual,
            stop_end_unix_time,
            comment: Some("HYB|sid=hybrid_imoexf|c=abc1230001|o=MR|r=SL".to_string()),
            instrument_group: None,
            check_duplicates: Some(true),
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::ProtectiveRepair)
        .with_symbol("IMOEXF")
    }

    fn stage5cg_cancel_intent() -> crate::BrokerNeutralHybridIntent {
        crate::BrokerNeutralHybridIntent::Cancel {
            order_id: BrokerOrderId::new("ORDER_TEST_0001"),
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::CancelCleanup)
        .with_symbol("IMOEXF")
    }

    fn stage5ci_ack(request_id: StrategyRequestId) -> broker_core::HybridRuntimeCommandAck {
        stage5ci_ack_with(
            request_id,
            broker_core::HybridRuntimeAckStatus::Accepted,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 10, 1)
                .single()
                .unwrap()
                .timestamp(),
        )
    }

    fn stage5ci_ack_with(
        request_id: StrategyRequestId,
        status: broker_core::HybridRuntimeAckStatus,
        processed_ts_utc: i64,
    ) -> broker_core::HybridRuntimeCommandAck {
        broker_core::HybridRuntimeCommandAck {
            request_id,
            status,
            broker_order_id: Some(BrokerOrderId::new("ORDER_TEST_ACK_0001")),
            error_code: None,
            error_message: None,
            processed_ts_utc,
        }
    }

    fn stage5ci_ack_record(
        total_sequence: u64,
        request_id: StrategyRequestId,
    ) -> Stage5cPaperAckRecord {
        Stage5cPaperAckRecord {
            total_sequence,
            ack: stage5ci_ack(request_id),
        }
    }

    fn stage5cj_position_event(
        total_sequence: u64,
        request_id: StrategyRequestId,
        qty: f64,
        source_ts_utc: i64,
    ) -> Stage5cPaperBrokerEventRecord {
        Stage5cPaperBrokerEventRecord {
            total_sequence,
            request_id,
            payload: Stage5cPaperBrokerEventPayload::Position(
                broker_core::HybridRuntimePositionEvent {
                    instrument: target(),
                    qty,
                    existing: true,
                    avg_price: 2227.5,
                    source_ts_utc,
                },
            ),
        }
    }

    fn stage5cj_attribution(role: &str) -> broker_core::HybridRuntimeAttribution {
        stage5cj_attribution_with_cycle(role, "abc1230001")
    }

    fn stage5cj_attribution_with_cycle(
        role: &str,
        cycle: &str,
    ) -> broker_core::HybridRuntimeAttribution {
        broker_core::HybridRuntimeAttribution::parse_source_comment(format!(
            "HYB|sid=hybrid_imoexf|c={cycle}|o=MR|r={role}"
        ))
        .unwrap()
    }

    fn stage5cj_order_event(
        total_sequence: u64,
        request_id: StrategyRequestId,
        order_id: BrokerOrderId,
        status: &str,
        source_ts_utc: i64,
    ) -> Stage5cPaperBrokerEventRecord {
        Stage5cPaperBrokerEventRecord {
            total_sequence,
            request_id,
            payload: Stage5cPaperBrokerEventPayload::Order(broker_core::HybridRuntimeOrderEvent {
                order_id,
                request_id: Some(request_id),
                instrument: target(),
                status: status.to_string(),
                side: "sell".to_string(),
                order_type: "limit".to_string(),
                qty: 1.0,
                filled_qty: if stage5cj_order_status_is_filled(status) {
                    1.0
                } else {
                    0.0
                },
                price: 2230.0,
                existing: true,
                attribution: Some(stage5cj_attribution("TP")),
                source_ts_utc,
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn stage5cj_order_event_with_role(
        total_sequence: u64,
        request_id: StrategyRequestId,
        order_id: BrokerOrderId,
        status: &str,
        side: &str,
        price: f64,
        role: &str,
        source_ts_utc: i64,
    ) -> Stage5cPaperBrokerEventRecord {
        Stage5cPaperBrokerEventRecord {
            total_sequence,
            request_id,
            payload: Stage5cPaperBrokerEventPayload::Order(broker_core::HybridRuntimeOrderEvent {
                order_id,
                request_id: Some(request_id),
                instrument: target(),
                status: status.to_string(),
                side: side.to_string(),
                order_type: "limit".to_string(),
                qty: 1.0,
                filled_qty: if stage5cj_order_status_is_filled(status) {
                    1.0
                } else {
                    0.0
                },
                price,
                existing: true,
                attribution: Some(stage5cj_attribution(role)),
                source_ts_utc,
            }),
        }
    }

    fn stage5cj_stop_event(
        total_sequence: u64,
        request_id: StrategyRequestId,
        exchange_order_id: BrokerOrderId,
        status: &str,
        end_ts_utc: i64,
        source_ts_utc: i64,
    ) -> Stage5cPaperBrokerEventRecord {
        Stage5cPaperBrokerEventRecord {
            total_sequence,
            request_id,
            payload: Stage5cPaperBrokerEventPayload::StopOrder(
                broker_core::HybridRuntimeStopOrderEvent {
                    stop_order_id: BrokerStopOrderId::new("STOP_TEST_0001"),
                    exchange_order_id: Some(exchange_order_id),
                    instrument: target(),
                    status: status.to_string(),
                    side: "sell".to_string(),
                    qty: 1.0,
                    filled_qty: 0.0,
                    stop_price: 2210.0,
                    price: 2209.5,
                    existing: true,
                    attribution: Some(stage5cj_attribution("SL")),
                    end_ts_utc: Some(end_ts_utc),
                    source_ts_utc,
                },
            ),
        }
    }

    fn stage5ci_entry_settled() -> (Stage5cSettledPaperStrategy, StrategyRequestId, i64) {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let expected_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            3,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            expected_request_id,
        );
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![stage5cg_market_intent(
                crate::BrokerNeutralOrderSide::Buy,
                crate::BrokerNeutralHybridIntentClass::Entry,
            )],
        ))
        .unwrap();
        (settled, expected_request_id, bar_close_ts)
    }

    fn stage5ci_exit_settled() -> (Stage5cSettledPaperStrategy, StrategyRequestId, i64) {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let expected_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            4,
        );
        let mut state = Strategy::state(&strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                last_position_qty,
                current_side,
                pending_exit_request_id,
                ..
            } => {
                *active_cycle_id = Some("abc1230001".to_string());
                *last_position_qty = 1.0;
                *current_side = Some(crate::hybrid_intraday::Side::Long);
                *pending_exit_request_id = Some(expected_request_id);
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(&mut strategy, state);
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![stage5cg_market_intent(
                crate::BrokerNeutralOrderSide::Sell,
                crate::BrokerNeutralHybridIntentClass::Exit,
            )],
        ))
        .unwrap();
        (settled, expected_request_id, bar_close_ts)
    }

    fn stage5ci_protective_settled() -> (
        Stage5cSettledPaperStrategy,
        StrategyRequestId,
        StrategyRequestId,
        i64,
    ) {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let tp_expected = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "place",
            bar_close_ts,
            0,
        );
        let sl_expected = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "create_stop_limit",
            bar_close_ts,
            5,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::ProtectiveRepair,
            tp_expected,
        );
        set_hybrid_pending_sl_request(&mut strategy, sl_expected);
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![
                stage5cg_place_intent(),
                stage5cg_stop_intent(bar_close_ts + 600),
            ],
        ))
        .unwrap();
        (settled, tp_expected, sl_expected, bar_close_ts)
    }

    fn stage5cj_place_entry_settled(
        side: crate::BrokerNeutralOrderSide,
        qty: f64,
    ) -> (Stage5cSettledPaperStrategy, StrategyRequestId, i64) {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "place",
            bar_close_ts,
            0,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            request_id,
        );
        let intent = crate::BrokerNeutralHybridIntent::Place {
            price: 2227.5,
            qty,
            side,
            comment: Some("HYB|sid=hybrid_imoexf|c=abc1230001|o=MR|r=ENTRY".to_string()),
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::Entry)
        .with_symbol("IMOEXF");
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![intent],
        ))
        .unwrap();
        (settled, request_id, bar_close_ts)
    }

    fn stage5cj_place_order_event(
        total_sequence: u64,
        request_id: StrategyRequestId,
        status: &str,
        side: &str,
        qty: f64,
        source_ts_utc: i64,
    ) -> Stage5cPaperBrokerEventRecord {
        let mut event = stage5cj_order_event_with_role(
            total_sequence,
            request_id,
            BrokerOrderId::new("ORDER_TEST_ACK_0001"),
            status,
            side,
            2227.5,
            "ENTRY",
            source_ts_utc,
        );
        if let Stage5cPaperBrokerEventPayload::Order(order) = &mut event.payload {
            order.qty = qty;
            order.filled_qty = if stage5cj_order_status_is_filled(status) {
                qty
            } else {
                0.0
            };
        }
        event
    }

    fn stage5cj_cleanup_cancel_settled() -> (Stage5cSettledPaperStrategy, StrategyRequestId, i64) {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "cancel:ORDER_TEST_0001",
            bar_close_ts,
            1,
        );
        let mut state = Strategy::state(&strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                current_owner,
                tp_order_id,
                ..
            } => {
                *active_cycle_id = Some("abc1230001".to_string());
                *current_owner = Some(crate::hybrid_intraday::Owner::MeanReversion);
                *tp_order_id = Some(BrokerOrderId::new("ORDER_TEST_0001"));
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(&mut strategy, state);
        let intent = crate::BrokerNeutralHybridIntent::Cancel {
            order_id: BrokerOrderId::new("ORDER_TEST_0001"),
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::CancelCleanup)
        .with_symbol("IMOEXF");
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![intent],
        ))
        .unwrap();
        (settled, request_id, bar_close_ts)
    }

    fn stage5cj_cleanup_delete_stop_settled(
    ) -> (Stage5cSettledPaperStrategy, StrategyRequestId, i64) {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "delete_stop_limit:STOP_TEST_0001",
            bar_close_ts,
            6,
        );
        let mut state = Strategy::state(&strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                current_owner,
                sl_stop_order_id,
                ..
            } => {
                *active_cycle_id = Some("abc1230001".to_string());
                *current_owner = Some(crate::hybrid_intraday::Owner::MeanReversion);
                *sl_stop_order_id = Some(BrokerStopOrderId::new("STOP_TEST_0001"));
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(&mut strategy, state);
        let intent = crate::BrokerNeutralHybridIntent::DeleteStopLimit {
            order_id: BrokerStopOrderId::new("STOP_TEST_0001"),
            side: Some(crate::BrokerNeutralOrderSide::Sell),
            check_duplicates: Some(true),
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::CancelCleanup)
        .with_symbol("IMOEXF");
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![intent],
        ))
        .unwrap();
        (settled, request_id, bar_close_ts)
    }

    #[test]
    fn stage5cg_settles_zero_intent_result_without_sink() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered(now);
        let (strategy, recovery_receipt) = recovered.into_parts();
        let settled = settle_stage5c_semantic_result(Stage5cSemanticBarResult {
            strategy,
            recovery_receipt,
            bar_close_ts: now.timestamp() + 600,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            execution_eligible: true,
            intents: Vec::new(),
            expected_attribution_by_request: HashMap::new(),
        })
        .unwrap();
        assert_eq!(settled.intent_batch().intent_count(), 0);
        assert!(settled.intent_batch().request_ids().is_empty());
        assert!(!settled.intent_batch().observation_only());
        assert!(!settled.intent_sink_attached());
        assert!(!settled.broker_transport_attached());
        let _ = Strategy::state(settled.strategy());
    }

    #[test]
    fn stage5cg_rejects_invalid_intent_before_settlement() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (strategy, recovery_receipt) = recovered.into_parts();
        let intent = crate::BrokerNeutralHybridIntent::Market {
            qty: -1.0,
            side: crate::BrokerNeutralOrderSide::Buy,
            fill_price: None,
            comment: None,
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::Entry)
        .with_symbol("IMOEXF");
        assert!(matches!(
            settle_stage5c_semantic_result(Stage5cSemanticBarResult {
                strategy,
                recovery_receipt,
                bar_close_ts: now.timestamp() + 600,
                origin: broker_core::HybridRuntimeBarOrigin::Live,
                execution_eligible: true,
                intents: vec![intent],
                expected_attribution_by_request: HashMap::new(),
            }),
            Err(Stage5cIntentSettlementError::InvalidQuantity)
        ));
    }

    #[test]
    fn stage5cg_live_entry_batch_id_matches_pending_entry_request_id() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered(now);
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = now.timestamp() + 600;
        let expected = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            3,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            expected,
        );
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![stage5cg_market_intent(
                crate::BrokerNeutralOrderSide::Buy,
                crate::BrokerNeutralHybridIntentClass::Entry,
            )],
        ))
        .unwrap();
        assert_eq!(settled.intent_batch().request_ids(), &[expected]);
        assert_eq!(
            settled.intent_batch().request_ids().first().copied(),
            match Strategy::state(settled.strategy()) {
                StrategyState::HybridIntradayRuntime {
                    pending_entry_request_id,
                    ..
                } => *pending_entry_request_id,
                StrategyState::Idle => None,
            }
        );
    }

    #[test]
    fn stage5cg_live_exit_batch_id_matches_pending_exit_request_id() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered(now);
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = now.timestamp() + 600;
        let expected = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            4,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Exit,
            expected,
        );
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![stage5cg_market_intent(
                crate::BrokerNeutralOrderSide::Sell,
                crate::BrokerNeutralHybridIntentClass::Exit,
            )],
        ))
        .unwrap();
        assert_eq!(settled.intent_batch().request_ids(), &[expected]);
    }

    #[test]
    fn stage5cg_protective_tp_sl_ids_match_wrapper_pending_ids() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered(now);
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = now.timestamp() + 600;
        let tp_expected = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "place",
            bar_close_ts,
            0,
        );
        let sl_expected = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "create_stop_limit",
            bar_close_ts,
            5,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::ProtectiveRepair,
            tp_expected,
        );
        set_hybrid_pending_sl_request(&mut strategy, sl_expected);
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![
                stage5cg_place_intent(),
                stage5cg_stop_intent(bar_close_ts + 600),
            ],
        ))
        .unwrap();
        assert_eq!(
            settled.intent_batch().request_ids(),
            &[tp_expected, sl_expected]
        );
    }

    #[test]
    fn stage5cg_replay_intent_is_blocked() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered(now);
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = now.timestamp() + 600;
        let expected = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            3,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            expected,
        );
        assert!(matches!(
            settle_stage5c_semantic_result(stage5cg_semantic_result(
                strategy,
                recovery_receipt,
                bar_close_ts,
                broker_core::HybridRuntimeBarOrigin::Replay,
                vec![stage5cg_market_intent(
                    crate::BrokerNeutralOrderSide::Buy,
                    crate::BrokerNeutralHybridIntentClass::Entry,
                )],
            )),
            Err(Stage5cIntentSettlementError::ReplayIntentNotExecutable)
        ));
    }

    #[test]
    fn stage5cg_source_request_id_collision_is_blocked_not_hidden_by_index() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered(now);
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = now.timestamp() + 600;
        let expected = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "place",
            bar_close_ts,
            0,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::ProtectiveRepair,
            expected,
        );
        assert!(matches!(
            settle_stage5c_semantic_result(stage5cg_semantic_result(
                strategy,
                recovery_receipt,
                bar_close_ts,
                broker_core::HybridRuntimeBarOrigin::Live,
                vec![stage5cg_place_intent(), stage5cg_place_intent()],
            )),
            Err(Stage5cIntentSettlementError::DuplicateRequestId)
        ));
    }

    #[test]
    fn stage5cg_nonzero_valid_intent_batch_preserves_state_fingerprint() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered(now);
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = now.timestamp() + 600;
        let expected = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            3,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            expected,
        );
        let expected_fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(Strategy::state(&strategy)).unwrap())
        );
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![stage5cg_market_intent(
                crate::BrokerNeutralOrderSide::Buy,
                crate::BrokerNeutralHybridIntentClass::Entry,
            )],
        ))
        .unwrap();
        assert_eq!(
            settled.intent_batch().state_fingerprint(),
            expected_fingerprint
        );
    }

    #[test]
    fn stage5ch_controlled_next_bar_requires_settled_input_and_accumulates_history() {
        let wall_now = Utc::now();
        let now = wall_now - chrono::Duration::hours(3);
        let recovered = empty_recovered_until(now, wall_now + chrono::Duration::hours(1));
        let (strategy, recovery_receipt) = recovered.into_parts();
        let first_close_ts = wall_now.timestamp().div_euclid(600) * 600 - 7_200;
        let settled = settle_stage5c_semantic_result(Stage5cSemanticBarResult {
            strategy,
            recovery_receipt,
            bar_close_ts: first_close_ts,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            execution_eligible: true,
            intents: Vec::new(),
            expected_attribution_by_request: HashMap::new(),
        })
        .unwrap();
        assert_eq!(settled.settled_batch_history().len(), 1);
        let next_close_ts = first_close_ts + 600;
        let accepted = accept_stage5c_semantic_bar(semantic_input(next_close_ts)).unwrap();
        let advanced = advance_stage5c_controlled_next_bar_at(
            settled,
            accepted,
            DateTime::<Utc>::from_timestamp(next_close_ts + 30, 0).unwrap(),
        )
        .unwrap();
        assert_eq!(advanced.intent_batch().bar_close_ts(), next_close_ts);
        assert_eq!(advanced.settled_batch_history().len(), 2);
        assert_eq!(
            advanced.settled_batch_history()[0].bar_close_ts,
            first_close_ts
        );
        assert_eq!(
            advanced.settled_batch_history()[1].bar_close_ts,
            next_close_ts
        );
        assert!(!advanced.intent_sink_attached());
        assert!(!advanced.broker_transport_attached());
        assert!(!advanced.timer_path_enabled());
    }

    #[test]
    fn stage5ch_rejects_non_monotonic_next_bar_before_callback() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered(now);
        let (strategy, recovery_receipt) = recovered.into_parts();
        let first_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let settled = settle_stage5c_semantic_result(Stage5cSemanticBarResult {
            strategy,
            recovery_receipt,
            bar_close_ts: first_close_ts,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            execution_eligible: true,
            intents: Vec::new(),
            expected_attribution_by_request: HashMap::new(),
        })
        .unwrap();
        let accepted = accept_stage5c_semantic_bar(semantic_input(first_close_ts)).unwrap();
        let failure = advance_stage5c_controlled_next_bar_at(
            settled,
            accepted,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 20, 30)
                .single()
                .unwrap(),
        )
        .expect_err("non-monotonic bar must be blocked before callback");
        assert_eq!(failure.reason(), Stage5cNextBarLoopError::NonMonotonicBar);
        assert!(failure.into_blocked().is_some());
    }

    #[test]
    fn stage5ch_nonzero_live_batch_blocks_next_bar_and_preserves_full_intent_batch() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let first_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let expected_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            first_close_ts,
            3,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            expected_request_id,
        );
        let expected_state_fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(Strategy::state(&strategy)).unwrap())
        );
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            first_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![stage5cg_market_intent(
                crate::BrokerNeutralOrderSide::Buy,
                crate::BrokerNeutralHybridIntentClass::Entry,
            )],
        ))
        .unwrap();
        assert_eq!(settled.intent_batch().intent_count(), 1);
        let next_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 20, 0)
            .single()
            .unwrap()
            .timestamp();
        let accepted = accept_stage5c_semantic_bar(semantic_input(next_close_ts)).unwrap();
        let failure = advance_stage5c_controlled_next_bar_at(
            settled,
            accepted,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 20, 30)
                .single()
                .unwrap(),
        )
        .expect_err("unresolved live intent batch must block the next bar");
        assert_eq!(
            failure.reason(),
            Stage5cNextBarLoopError::UnresolvedIntentBatch
        );
        let blocked = failure
            .into_blocked()
            .expect("unresolved batch must preserve settled type-state");
        assert_eq!(
            blocked.reason(),
            Stage5cNextBarLoopError::UnresolvedIntentBatch
        );
        assert_eq!(blocked.settled().intent_batch().intent_count(), 1);
        assert_eq!(
            blocked.settled().intent_batch().request_ids(),
            &[expected_request_id]
        );
        assert_eq!(
            blocked.settled().intent_batch().record_request_ids(),
            vec![expected_request_id]
        );
        assert_eq!(
            blocked.settled().intent_batch().intent_classes(),
            vec![crate::BrokerNeutralHybridIntentClass::Entry]
        );
        assert_eq!(
            blocked.settled().intent_batch().state_fingerprint(),
            expected_state_fingerprint
        );
        assert_eq!(
            match Strategy::state(blocked.settled().strategy()) {
                StrategyState::HybridIntradayRuntime {
                    pending_entry_request_id,
                    ..
                } => *pending_entry_request_id,
                StrategyState::Idle => None,
            },
            Some(expected_request_id)
        );
    }

    #[test]
    fn stage5ch_unresolved_batch_does_not_invoke_on_broker_bar_or_change_strategy_state() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let first_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let expected_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            first_close_ts,
            3,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            expected_request_id,
        );
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            first_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![stage5cg_market_intent(
                crate::BrokerNeutralOrderSide::Buy,
                crate::BrokerNeutralHybridIntentClass::Entry,
            )],
        ))
        .unwrap();
        let before_state = serde_json::to_value(Strategy::state(settled.strategy())).unwrap();
        let next_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 20, 0)
            .single()
            .unwrap()
            .timestamp();
        let accepted = accept_stage5c_semantic_bar(semantic_input(next_close_ts)).unwrap();
        let blocked = advance_stage5c_controlled_next_bar_at(
            settled,
            accepted,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 20, 30)
                .single()
                .unwrap(),
        )
        .expect_err("unresolved live intent batch must block the next bar")
        .into_blocked()
        .expect("blocked unresolved batch must return original settled state");
        let after_state =
            serde_json::to_value(Strategy::state(blocked.settled().strategy())).unwrap();
        assert_eq!(after_state, before_state);
        assert_eq!(
            blocked.settled().intent_batch().bar_close_ts(),
            first_close_ts
        );
    }

    #[test]
    fn stage5ch_cancel_only_batch_is_still_unresolved() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (strategy, recovery_receipt) = recovered.into_parts();
        let first_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            first_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![stage5cg_cancel_intent()],
        ))
        .unwrap();
        assert_eq!(settled.intent_batch().intent_count(), 1);
        assert!(!settled.intent_batch().has_actionable_intents());
        let next_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 20, 0)
            .single()
            .unwrap()
            .timestamp();
        let accepted = accept_stage5c_semantic_bar(semantic_input(next_close_ts)).unwrap();
        let failure = advance_stage5c_controlled_next_bar_at(
            settled,
            accepted,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 20, 30)
                .single()
                .unwrap(),
        )
        .expect_err("cancel-only batch still requires lifecycle settlement");
        assert_eq!(
            failure.reason(),
            Stage5cNextBarLoopError::UnresolvedIntentBatch
        );
        assert_eq!(
            failure
                .into_blocked()
                .expect("cancel-only block must preserve batch")
                .settled()
                .intent_batch()
                .intent_classes(),
            vec![crate::BrokerNeutralHybridIntentClass::CancelCleanup]
        );
    }

    #[test]
    fn stage5ch_zero_intent_batch_allows_next_bar() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (strategy, recovery_receipt) = recovered.into_parts();
        let first_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let settled = settle_stage5c_semantic_result(Stage5cSemanticBarResult {
            strategy,
            recovery_receipt,
            bar_close_ts: first_close_ts,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            execution_eligible: true,
            intents: Vec::new(),
            expected_attribution_by_request: HashMap::new(),
        })
        .unwrap();
        let next_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 20, 0)
            .single()
            .unwrap()
            .timestamp();
        let accepted = accept_stage5c_semantic_bar(semantic_input(next_close_ts)).unwrap();
        let advanced = advance_stage5c_controlled_next_bar_at(
            settled,
            accepted,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 20, 30)
                .single()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(advanced.intent_batch().bar_close_ts(), next_close_ts);
    }

    #[test]
    fn stage5ch_broker_truth_expiry_block_preserves_settled_state() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered(now);
        let (strategy, recovery_receipt) = recovered.into_parts();
        let first_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let settled = settle_stage5c_semantic_result(Stage5cSemanticBarResult {
            strategy,
            recovery_receipt,
            bar_close_ts: first_close_ts,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            execution_eligible: true,
            intents: Vec::new(),
            expected_attribution_by_request: HashMap::new(),
        })
        .unwrap();
        let next_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 20, 0)
            .single()
            .unwrap()
            .timestamp();
        let accepted = accept_stage5c_semantic_bar(semantic_input(next_close_ts)).unwrap();
        let failure = advance_stage5c_controlled_next_bar_at(
            settled,
            accepted,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 31)
                .single()
                .unwrap(),
        )
        .expect_err("expired broker truth must block before callback");
        assert_eq!(
            failure.reason(),
            Stage5cNextBarLoopError::Semantic(Stage5cSemanticBarError::BrokerTruthExpired)
        );
        let blocked = failure
            .into_blocked()
            .expect("preflight expiry must preserve settled state");
        assert_eq!(
            blocked.settled().intent_batch().bar_close_ts(),
            first_close_ts
        );
    }

    #[test]
    fn stage5ch_rechecks_broker_truth_expiry() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered(now);
        let (strategy, recovery_receipt) = recovered.into_parts();
        let first_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let settled = settle_stage5c_semantic_result(Stage5cSemanticBarResult {
            strategy,
            recovery_receipt,
            bar_close_ts: first_close_ts,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            execution_eligible: true,
            intents: Vec::new(),
            expected_attribution_by_request: HashMap::new(),
        })
        .unwrap();
        let next_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 20, 0)
            .single()
            .unwrap()
            .timestamp();
        let accepted = accept_stage5c_semantic_bar(semantic_input(next_close_ts)).unwrap();
        assert_eq!(
            advance_stage5c_controlled_next_bar_at(
                settled,
                accepted,
                Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 31)
                    .single()
                    .unwrap(),
            )
            .expect_err("expired broker truth must block")
            .reason(),
            Stage5cNextBarLoopError::Semantic(Stage5cSemanticBarError::BrokerTruthExpired)
        );
    }

    #[test]
    fn stage5ci_resolves_nonzero_batch_by_exact_ack_without_sink_or_transport() {
        let (settled, expected_request_id, _) = stage5ci_entry_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, expected_request_id)],
            },
        )
        .unwrap();
        let summary = resolved.resolved_batch_summary();
        assert_eq!(summary.intent_count, 1);
        assert_eq!(summary.request_ids, vec![expected_request_id]);
        assert_eq!(
            resolved.full_resolved_batch().record_request_ids(),
            vec![expected_request_id]
        );
        assert_eq!(resolved.ack_outcomes().len(), 1);
        assert_eq!(resolved.ack_outcomes()[0].total_sequence, 1);
        assert_eq!(resolved.ack_outcomes()[0].request_id, expected_request_id);
        assert_eq!(
            resolved.ack_outcomes()[0].status,
            broker_core::HybridRuntimeAckStatus::Accepted
        );
        assert_eq!(
            resolved.ack_outcomes()[0].broker_order_id,
            Some(BrokerOrderId::new("ORDER_TEST_ACK_0001"))
        );
        assert_eq!(resolved.settled_batch_history().len(), 1);
        assert!(!resolved.intent_sink_attached());
        assert!(!resolved.broker_transport_attached());
        assert!(!resolved.timer_path_enabled());
        assert_eq!(
            match Strategy::state(resolved.strategy()) {
                StrategyState::HybridIntradayRuntime {
                    pending_entry_request_id,
                    ..
                } => *pending_entry_request_id,
                StrategyState::Idle => None,
            },
            Some(expected_request_id),
            "accepted ACK alone is not a fill/position lifecycle and must not fake flat/filled state"
        );
    }

    #[test]
    fn stage5ci_rejects_missing_unknown_and_duplicate_ack() {
        let (settled_for_missing, _, bar_close_ts) = stage5ci_entry_settled();
        let blocked = resolve_stage5c_paper_intent_lifecycle(
            settled_for_missing,
            Stage5cPaperIntentLifecycleInput {
                ack_records: Vec::new(),
            },
        )
        .expect_err("missing ACK must preserve settled type-state")
        .into_blocked()
        .expect("missing ACK is a recoverable preflight block");
        assert_eq!(
            blocked.reason(),
            Stage5cPaperIntentLifecycleError::MissingAck
        );
        assert_eq!(blocked.settled().intent_batch().intent_count(), 1);
        let unknown = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            4,
        );
        let (settled_for_unknown, _, _) = stage5ci_entry_settled();
        assert!(matches!(
            resolve_stage5c_paper_intent_lifecycle(
                settled_for_unknown,
                Stage5cPaperIntentLifecycleInput {
                    ack_records: vec![stage5ci_ack_record(1, unknown)]
                },
            ),
            Err(Stage5cPaperIntentLifecycleFailure::Blocked(_))
        ));
        let (settled_for_duplicate, expected_request_id, _) = stage5ci_entry_settled();
        assert!(matches!(
            resolve_stage5c_paper_intent_lifecycle(
                settled_for_duplicate,
                Stage5cPaperIntentLifecycleInput {
                    ack_records: vec![
                        stage5ci_ack_record(1, expected_request_id),
                        stage5ci_ack_record(2, expected_request_id)
                    ],
                },
            ),
            Err(Stage5cPaperIntentLifecycleFailure::Blocked(_))
        ));
    }

    #[test]
    fn stage5ci_rejects_state_fingerprint_mismatch() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let expected_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            3,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            expected_request_id,
        );
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![stage5cg_market_intent(
                crate::BrokerNeutralOrderSide::Buy,
                crate::BrokerNeutralHybridIntentClass::Entry,
            )],
        ))
        .unwrap();
        let (mut strategy, recovery_receipt, batch) = settled.into_parts();
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::deterministic_request_id(
                "hybrid_imoexf",
                "ACC_TEST_0001",
                "IMOEXF",
                "market",
                bar_close_ts + 600,
                3,
            ),
        );
        let drifted = Stage5cSettledPaperStrategy {
            strategy,
            recovery_receipt,
            batch: Stage5cPaperIntentBatch {
                strategy_id: batch.strategy_id.clone(),
                account_id: batch.account_id.clone(),
                instrument: batch.instrument.clone(),
                bar_close_ts: batch.bar_close_ts,
                state_fingerprint: batch.state_fingerprint.clone(),
                request_ids: batch.request_ids.clone(),
                records: batch.records.clone(),
                observation_only: batch.observation_only,
            },
            settled_batch_history: vec![stage5ch_batch_summary(&batch)],
        };
        assert!(matches!(
            resolve_stage5c_paper_intent_lifecycle(
                drifted,
                Stage5cPaperIntentLifecycleInput {
                    ack_records: vec![stage5ci_ack_record(1, expected_request_id)]
                },
            ),
            Err(Stage5cPaperIntentLifecycleFailure::Blocked(_))
        ));
    }

    #[test]
    fn stage5ci_same_ack_set_has_one_canonical_application_order() {
        let (settled_a, tp_request_id, sl_request_id, bar_close_ts) = stage5ci_protective_settled();
        let (settled_b, _, _, _) = stage5ci_protective_settled();
        let tp_ack = Stage5cPaperAckRecord {
            total_sequence: 1,
            ack: stage5ci_ack_with(
                tp_request_id,
                broker_core::HybridRuntimeAckStatus::Rejected,
                bar_close_ts + 100,
            ),
        };
        let sl_ack = Stage5cPaperAckRecord {
            total_sequence: 2,
            ack: stage5ci_ack_with(
                sl_request_id,
                broker_core::HybridRuntimeAckStatus::Rejected,
                bar_close_ts + 200,
            ),
        };
        let resolved_a = resolve_stage5c_paper_intent_lifecycle(
            settled_a,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![tp_ack.clone(), sl_ack.clone()],
            },
        )
        .unwrap();
        let resolved_b = resolve_stage5c_paper_intent_lifecycle(
            settled_b,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![sl_ack, tp_ack],
            },
        )
        .unwrap();
        assert_eq!(
            resolved_a.post_lifecycle_state_fingerprint(),
            resolved_b.post_lifecycle_state_fingerprint()
        );
        assert_eq!(
            resolved_b
                .ack_outcomes()
                .iter()
                .map(|outcome| outcome.total_sequence)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[test]
    fn stage5ci_rejects_duplicate_sequence_and_ack_before_intent_bar() {
        let (settled_duplicate, expected_request_id, _) = stage5ci_entry_settled();
        let duplicate = resolve_stage5c_paper_intent_lifecycle(
            settled_duplicate,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![
                    stage5ci_ack_record(1, expected_request_id),
                    stage5ci_ack_record(1, expected_request_id),
                ],
            },
        )
        .expect_err("duplicate sequence must be blocked")
        .into_blocked()
        .expect("duplicate sequence is a recoverable preflight block");
        assert_eq!(
            duplicate.reason(),
            Stage5cPaperIntentLifecycleError::DuplicateSequence
        );

        let (settled_early, expected_request_id, bar_close_ts) = stage5ci_entry_settled();
        let early = resolve_stage5c_paper_intent_lifecycle(
            settled_early,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![Stage5cPaperAckRecord {
                    total_sequence: 1,
                    ack: stage5ci_ack_with(
                        expected_request_id,
                        broker_core::HybridRuntimeAckStatus::Accepted,
                        bar_close_ts - 1,
                    ),
                }],
            },
        )
        .expect_err("ACK before intent bar must be blocked")
        .into_blocked()
        .expect("early ACK is a recoverable preflight block");
        assert_eq!(
            early.reason(),
            Stage5cPaperIntentLifecycleError::AckTimestampBeforeIntentBar
        );
    }

    #[test]
    fn stage5ci_rejects_empty_intent_batch() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (strategy, recovery_receipt) = recovered.into_parts();
        let settled = settle_stage5c_semantic_result(Stage5cSemanticBarResult {
            strategy,
            recovery_receipt,
            bar_close_ts: Utc
                .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
                .single()
                .unwrap()
                .timestamp(),
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            execution_eligible: true,
            intents: Vec::new(),
            expected_attribution_by_request: HashMap::new(),
        })
        .unwrap();
        assert!(matches!(
            resolve_stage5c_paper_intent_lifecycle(
                settled,
                Stage5cPaperIntentLifecycleInput {
                    ack_records: Vec::new()
                },
            ),
            Err(Stage5cPaperIntentLifecycleFailure::Blocked(_))
        ));
    }

    #[test]
    fn stage5cj_market_ack_requires_position_fill_event_before_next_type_state() {
        let (settled, request_id, _) = stage5ci_entry_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        let missing = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: Vec::new(),
            },
        )
        .expect_err("accepted market ACK is not fill evidence");
        assert_eq!(
            missing.reason(),
            Stage5cPaperBrokerLifecycleError::MissingExpectedBrokerEvent
        );

        let (settled, request_id, _) = stage5ci_entry_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![stage5cj_position_event(
                    2,
                    request_id,
                    1.0,
                    Utc.with_ymd_and_hms(2026, 7, 13, 9, 10, 2)
                        .single()
                        .unwrap()
                        .timestamp(),
                )],
            },
        )
        .unwrap();
        assert_eq!(broker_resolved.broker_event_count(), 1);
        assert!(!broker_resolved.intent_sink_attached());
        assert!(!broker_resolved.broker_transport_attached());
        assert!(!broker_resolved.timer_path_enabled());
        assert_eq!(
            match Strategy::state(broker_resolved.strategy()) {
                StrategyState::HybridIntradayRuntime {
                    last_position_qty, ..
                } => *last_position_qty,
                StrategyState::Idle => 0.0,
            },
            1.0,
            "position lifecycle is fill evidence and updates the broker-neutral position truth"
        );
        assert_eq!(
            match Strategy::state(broker_resolved.strategy()) {
                StrategyState::HybridIntradayRuntime {
                    pending_entry_request_id,
                    ..
                } => *pending_entry_request_id,
                StrategyState::Idle => None,
            },
            Some(request_id),
            "facade must not invent pending cleanup semantics that source runtime does not expose"
        );
    }

    #[test]
    fn stage5cj_market_exit_accepts_flat_position_event_and_rejects_nonflat() {
        let (settled, request_id, bar_close_ts) = stage5ci_exit_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![stage5cj_position_event(
                    2,
                    request_id,
                    0.0,
                    bar_close_ts + 2,
                )],
            },
        )
        .unwrap();
        assert_eq!(
            match Strategy::state(broker_resolved.strategy()) {
                StrategyState::HybridIntradayRuntime {
                    last_position_qty,
                    pending_exit_request_id,
                    ..
                } => (*last_position_qty, *pending_exit_request_id),
                StrategyState::Idle => (f64::NAN, None),
            },
            (0.0, None)
        );

        let (settled, request_id, bar_close_ts) = stage5ci_exit_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        assert_eq!(
            resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![stage5cj_position_event(
                        2,
                        request_id,
                        1.0,
                        bar_close_ts + 2
                    )]
                },
            )
            .expect_err("exit lifecycle must finish flat")
            .reason(),
            Stage5cPaperBrokerLifecycleError::PositionEventRequiresMarketIntent
        );
    }

    #[test]
    fn stage5cj_market_entry_checks_position_direction() {
        let (settled, request_id, bar_close_ts) = stage5ci_entry_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        assert_eq!(
            resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![stage5cj_position_event(
                        2,
                        request_id,
                        -1.0,
                        bar_close_ts + 2
                    )]
                },
            )
            .expect_err("buy entry cannot settle into short broker position")
            .reason(),
            Stage5cPaperBrokerLifecycleError::PositionSideMismatch
        );

        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            4,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            request_id,
        );
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![stage5cg_market_intent(
                crate::BrokerNeutralOrderSide::Sell,
                crate::BrokerNeutralHybridIntentClass::Entry,
            )],
        ))
        .unwrap();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        assert_eq!(
            resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![stage5cj_position_event(
                        2,
                        request_id,
                        1.0,
                        bar_close_ts + 2
                    )]
                },
            )
            .expect_err("sell entry cannot settle into long broker position")
            .reason(),
            Stage5cPaperBrokerLifecycleError::PositionSideMismatch
        );
    }

    #[test]
    fn stage5cj_rejected_ack_expects_no_broker_state_event() {
        let (settled, request_id, bar_close_ts) = stage5ci_entry_settled();
        let rejected_ack = Stage5cPaperAckRecord {
            total_sequence: 1,
            ack: stage5ci_ack_with(
                request_id,
                broker_core::HybridRuntimeAckStatus::Rejected,
                bar_close_ts + 1,
            ),
        };
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![rejected_ack],
            },
        )
        .unwrap();
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: Vec::new(),
            },
        )
        .unwrap();
        assert_eq!(broker_resolved.broker_event_count(), 0);

        let (settled, request_id, bar_close_ts) = stage5ci_entry_settled();
        let rejected_ack = Stage5cPaperAckRecord {
            total_sequence: 1,
            ack: stage5ci_ack_with(
                request_id,
                broker_core::HybridRuntimeAckStatus::Rejected,
                bar_close_ts + 1,
            ),
        };
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![rejected_ack],
            },
        )
        .unwrap();
        assert_eq!(
            resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![stage5cj_position_event(
                        2,
                        request_id,
                        1.0,
                        bar_close_ts + 2
                    )]
                },
            )
            .expect_err("terminal ACK must not accept a broker-state event")
            .reason(),
            Stage5cPaperBrokerLifecycleError::EventForTerminalAck
        );
    }

    #[test]
    fn stage5cj_order_and_stop_events_are_applied_in_canonical_total_sequence() {
        let (settled_a, tp_request_id, sl_request_id, bar_close_ts) = stage5ci_protective_settled();
        let (settled_b, _, _, _) = stage5ci_protective_settled();
        let tp_ack = Stage5cPaperAckRecord {
            total_sequence: 1,
            ack: stage5ci_ack_with(
                tp_request_id,
                broker_core::HybridRuntimeAckStatus::Accepted,
                bar_close_ts + 1,
            ),
        };
        let sl_ack = Stage5cPaperAckRecord {
            total_sequence: 2,
            ack: stage5ci_ack_with(
                sl_request_id,
                broker_core::HybridRuntimeAckStatus::Accepted,
                bar_close_ts + 1,
            ),
        };
        let resolved_a = resolve_stage5c_paper_intent_lifecycle(
            settled_a,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![tp_ack.clone(), sl_ack.clone()],
            },
        )
        .unwrap();
        let resolved_b = resolve_stage5c_paper_intent_lifecycle(
            settled_b,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![sl_ack, tp_ack],
            },
        )
        .unwrap();
        let order_event = stage5cj_order_event(
            3,
            tp_request_id,
            BrokerOrderId::new("ORDER_TEST_ACK_0001"),
            "filled",
            bar_close_ts + 2,
        );
        let stop_event = stage5cj_stop_event(
            4,
            sl_request_id,
            BrokerOrderId::new("ORDER_TEST_ACK_0001"),
            "working",
            bar_close_ts + 600,
            bar_close_ts + 3,
        );
        let broker_a = resolve_stage5c_paper_broker_lifecycle(
            resolved_a,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![order_event.clone(), stop_event.clone()],
            },
        )
        .unwrap();
        let broker_b = resolve_stage5c_paper_broker_lifecycle(
            resolved_b,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![stop_event, order_event],
            },
        )
        .unwrap();
        assert_eq!(
            broker_a.post_broker_lifecycle_state_fingerprint(),
            broker_b.post_broker_lifecycle_state_fingerprint()
        );
        assert_eq!(broker_b.broker_event_count(), 2);
        assert_eq!(broker_b.remaining_lifecycle_expectations().len(), 2);
    }

    #[test]
    fn stage5cj_place_lifecycle_accepts_working_then_filled_and_preserves_full_batch() {
        let (settled, tp_request_id, sl_request_id, bar_close_ts) = stage5ci_protective_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![
                    Stage5cPaperAckRecord {
                        total_sequence: 1,
                        ack: stage5ci_ack_with(
                            tp_request_id,
                            broker_core::HybridRuntimeAckStatus::Accepted,
                            bar_close_ts + 1,
                        ),
                    },
                    Stage5cPaperAckRecord {
                        total_sequence: 2,
                        ack: stage5ci_ack_with(
                            sl_request_id,
                            broker_core::HybridRuntimeAckStatus::Rejected,
                            bar_close_ts + 1,
                        ),
                    },
                ],
            },
        )
        .unwrap();
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![
                    stage5cj_order_event(
                        4,
                        tp_request_id,
                        BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                        "filled",
                        bar_close_ts + 3,
                    ),
                    stage5cj_order_event(
                        3,
                        tp_request_id,
                        BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                        "working",
                        bar_close_ts + 2,
                    ),
                    stage5cj_position_event(5, tp_request_id, 0.0, bar_close_ts + 4),
                ],
            },
        )
        .unwrap();
        assert_eq!(broker_resolved.broker_event_count(), 3);
        assert!(broker_resolved
            .remaining_lifecycle_expectations()
            .is_empty());
        assert_eq!(broker_resolved.full_resolved_batch().intent_count(), 2);
    }

    #[test]
    fn stage5cj_order_and_stop_events_require_valid_attribution() {
        let (settled, tp_request_id, sl_request_id, bar_close_ts) = stage5ci_protective_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![
                    Stage5cPaperAckRecord {
                        total_sequence: 1,
                        ack: stage5ci_ack_with(
                            tp_request_id,
                            broker_core::HybridRuntimeAckStatus::Accepted,
                            bar_close_ts + 1,
                        ),
                    },
                    Stage5cPaperAckRecord {
                        total_sequence: 2,
                        ack: stage5ci_ack_with(
                            sl_request_id,
                            broker_core::HybridRuntimeAckStatus::Rejected,
                            bar_close_ts + 1,
                        ),
                    },
                ],
            },
        )
        .unwrap();
        let mut event = stage5cj_order_event(
            3,
            tp_request_id,
            BrokerOrderId::new("ORDER_TEST_ACK_0001"),
            "working",
            bar_close_ts + 2,
        );
        if let Stage5cPaperBrokerEventPayload::Order(order) = &mut event.payload {
            order.attribution = None;
        }
        assert_eq!(
            resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![event]
                },
            )
            .expect_err("source wrapper would ignore unattributed order events")
            .reason(),
            Stage5cPaperBrokerLifecycleError::AttributionMissing
        );
    }

    #[test]
    fn stage5cj_blocks_wrong_event_kind_and_broker_order_mismatch() {
        let (settled, request_id, bar_close_ts) = stage5ci_entry_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        assert_eq!(
            resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![stage5cj_order_event(
                        2,
                        request_id,
                        BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                        "filled",
                        bar_close_ts + 2
                    )]
                },
            )
            .expect_err("market intent must not resolve through order event")
            .reason(),
            Stage5cPaperBrokerLifecycleError::UnexpectedBrokerEventKind
        );

        let (settled, tp_request_id, _, bar_close_ts) = stage5ci_protective_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![
                    Stage5cPaperAckRecord {
                        total_sequence: 1,
                        ack: stage5ci_ack_with(
                            tp_request_id,
                            broker_core::HybridRuntimeAckStatus::Accepted,
                            bar_close_ts + 1,
                        ),
                    },
                    Stage5cPaperAckRecord {
                        total_sequence: 2,
                        ack: stage5ci_ack_with(
                            crate::deterministic_request_id(
                                "hybrid_imoexf",
                                "ACC_TEST_0001",
                                "IMOEXF",
                                "create_stop_limit",
                                bar_close_ts,
                                5,
                            ),
                            broker_core::HybridRuntimeAckStatus::Rejected,
                            bar_close_ts + 1,
                        ),
                    },
                ],
            },
        )
        .unwrap();
        assert_eq!(
            resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![stage5cj_order_event(
                        3,
                        tp_request_id,
                        BrokerOrderId::new("ORDER_TEST_OTHER"),
                        "filled",
                        bar_close_ts + 2
                    )]
                },
            )
            .expect_err("broker order id must match ACK outcome")
            .reason(),
            Stage5cPaperBrokerLifecycleError::BrokerOrderIdMismatch
        );
    }

    #[test]
    fn stage5cj_deduplicates_identical_events_and_blocks_conflicting_duplicate() {
        let (settled, request_id, bar_close_ts) = stage5ci_entry_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        let event = stage5cj_position_event(2, request_id, 1.0, bar_close_ts + 2);
        let duplicate = Stage5cPaperBrokerEventRecord {
            total_sequence: 3,
            ..event.clone()
        };
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![duplicate, event],
            },
        )
        .unwrap();
        assert_eq!(broker_resolved.broker_event_count(), 1);

        let (settled, request_id, bar_close_ts) = stage5ci_entry_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        assert_eq!(
            resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![
                        stage5cj_position_event(2, request_id, 1.0, bar_close_ts + 2),
                        stage5cj_position_event(3, request_id, 2.0, bar_close_ts + 2)
                    ]
                },
            )
            .expect_err("same request with different payload is conflicting duplicate")
            .reason(),
            Stage5cPaperBrokerLifecycleError::ConflictingDuplicateEvent
        );
    }

    #[test]
    fn stage5cj_marketable_limit_entry_exit_accept_source_roles_and_position_confirmation() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let entry_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "place",
            bar_close_ts,
            0,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            entry_request_id,
        );
        let entry_intent = crate::BrokerNeutralHybridIntent::Place {
            price: 2230.0,
            qty: 1.0,
            side: crate::BrokerNeutralOrderSide::Buy,
            comment: Some("HYB|sid=hybrid_imoexf|c=abc1230001|o=MR|r=ENTRY".to_string()),
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::Entry)
        .with_symbol("IMOEXF");
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![entry_intent],
        ))
        .unwrap();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, entry_request_id)],
            },
        )
        .unwrap();
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![
                    stage5cj_order_event_with_role(
                        2,
                        entry_request_id,
                        BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                        "filled",
                        "buy",
                        2230.0,
                        "ENTRY",
                        bar_close_ts + 2,
                    ),
                    stage5cj_position_event(3, entry_request_id, 1.0, bar_close_ts + 3),
                ],
            },
        )
        .unwrap();
        assert!(broker_resolved
            .remaining_lifecycle_expectations()
            .is_empty());

        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let exit_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "place",
            bar_close_ts,
            0,
        );
        let mut state = Strategy::state(&strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                last_position_qty,
                current_side,
                pending_exit_request_id,
                ..
            } => {
                *active_cycle_id = Some("abc1230001".to_string());
                *last_position_qty = 1.0;
                *current_side = Some(crate::hybrid_intraday::Side::Long);
                *pending_exit_request_id = Some(exit_request_id);
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(&mut strategy, state);
        let exit_intent = crate::BrokerNeutralHybridIntent::Place {
            price: 2220.0,
            qty: 1.0,
            side: crate::BrokerNeutralOrderSide::Sell,
            comment: Some("HYB|sid=hybrid_imoexf|c=abc1230001|o=MR|r=EXIT".to_string()),
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::Exit)
        .with_symbol("IMOEXF");
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Exit,
            exit_request_id,
        );
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![exit_intent],
        ))
        .unwrap();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, exit_request_id)],
            },
        )
        .unwrap();
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![
                    stage5cj_order_event_with_role(
                        2,
                        exit_request_id,
                        BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                        "filled",
                        "sell",
                        2220.0,
                        "EXIT",
                        bar_close_ts + 2,
                    ),
                    stage5cj_position_event(3, exit_request_id, 0.0, bar_close_ts + 3),
                ],
            },
        )
        .unwrap();
        assert!(broker_resolved
            .remaining_lifecycle_expectations()
            .is_empty());
    }

    #[test]
    fn stage5cj_tp_cancel_accepts_original_tp_attribution_and_wrong_cycle_blocks() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let cancel_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "cancel:ORDER_TEST_0001",
            bar_close_ts,
            1,
        );
        let cancel_intent = crate::BrokerNeutralHybridIntent::Cancel {
            order_id: BrokerOrderId::new("ORDER_TEST_0001"),
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::CancelCleanup)
        .with_symbol("IMOEXF");
        let mut state = Strategy::state(&strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                current_owner,
                tp_order_id,
                ..
            } => {
                *active_cycle_id = Some("abc1230001".to_string());
                *current_owner = Some(crate::hybrid_intraday::Owner::MeanReversion);
                *tp_order_id = Some(BrokerOrderId::new("ORDER_TEST_0001"));
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(&mut strategy, state);
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![cancel_intent],
        ))
        .unwrap();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![Stage5cPaperAckRecord {
                    total_sequence: 1,
                    ack: broker_core::HybridRuntimeCommandAck {
                        broker_order_id: Some(BrokerOrderId::new("ORDER_TEST_0001")),
                        ..stage5ci_ack(cancel_request_id)
                    },
                }],
            },
        )
        .unwrap();
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![stage5cj_order_event_with_role(
                    2,
                    cancel_request_id,
                    BrokerOrderId::new("ORDER_TEST_0001"),
                    "canceled",
                    "sell",
                    2230.0,
                    "TP",
                    bar_close_ts + 2,
                )],
            },
        )
        .unwrap();
        assert!(broker_resolved
            .remaining_lifecycle_expectations()
            .is_empty());

        let (settled, tp_request_id, _, bar_close_ts) = stage5ci_protective_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![
                    Stage5cPaperAckRecord {
                        total_sequence: 1,
                        ack: stage5ci_ack_with(
                            tp_request_id,
                            broker_core::HybridRuntimeAckStatus::Accepted,
                            bar_close_ts + 1,
                        ),
                    },
                    Stage5cPaperAckRecord {
                        total_sequence: 2,
                        ack: stage5ci_ack_with(
                            crate::deterministic_request_id(
                                "hybrid_imoexf",
                                "ACC_TEST_0001",
                                "IMOEXF",
                                "create_stop_limit",
                                bar_close_ts,
                                5,
                            ),
                            broker_core::HybridRuntimeAckStatus::Rejected,
                            bar_close_ts + 1,
                        ),
                    },
                ],
            },
        )
        .unwrap();
        let mut event = stage5cj_order_event(
            3,
            tp_request_id,
            BrokerOrderId::new("ORDER_TEST_ACK_0001"),
            "working",
            bar_close_ts + 2,
        );
        if let Stage5cPaperBrokerEventPayload::Order(order) = &mut event.payload {
            order.attribution = Some(stage5cj_attribution_with_cycle("TP", "deadbeef01"));
        }
        assert_eq!(
            resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![event]
                },
            )
            .expect_err("wrong HYB cycle must be blocked before source callback")
            .reason(),
            Stage5cPaperBrokerLifecycleError::AttributionCycleMismatch
        );
    }

    #[test]
    fn stage5cj_triggered_and_executed_stop_require_position_confirmation() {
        for status in ["triggered", "executed"] {
            let (settled, tp_request_id, sl_request_id, bar_close_ts) =
                stage5ci_protective_settled();
            let resolved = resolve_stage5c_paper_intent_lifecycle(
                settled,
                Stage5cPaperIntentLifecycleInput {
                    ack_records: vec![
                        Stage5cPaperAckRecord {
                            total_sequence: 1,
                            ack: stage5ci_ack_with(
                                tp_request_id,
                                broker_core::HybridRuntimeAckStatus::Rejected,
                                bar_close_ts + 1,
                            ),
                        },
                        Stage5cPaperAckRecord {
                            total_sequence: 2,
                            ack: stage5ci_ack_with(
                                sl_request_id,
                                broker_core::HybridRuntimeAckStatus::Accepted,
                                bar_close_ts + 1,
                            ),
                        },
                    ],
                },
            )
            .unwrap();
            let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![stage5cj_stop_event(
                        3,
                        sl_request_id,
                        BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                        status,
                        bar_close_ts + 600,
                        bar_close_ts + 2,
                    )],
                },
            )
            .unwrap();
            assert_eq!(
                broker_resolved.remaining_lifecycle_expectations()[0].expected_event_kind,
                Stage5cPaperBrokerEventKind::Position
            );
        }
    }

    #[test]
    fn stage5cj_canceled_and_rejected_stop_are_terminal_without_position() {
        for status in ["canceled", "rejected"] {
            let (settled, tp_request_id, sl_request_id, bar_close_ts) =
                stage5ci_protective_settled();
            let resolved = resolve_stage5c_paper_intent_lifecycle(
                settled,
                Stage5cPaperIntentLifecycleInput {
                    ack_records: vec![
                        Stage5cPaperAckRecord {
                            total_sequence: 1,
                            ack: stage5ci_ack_with(
                                tp_request_id,
                                broker_core::HybridRuntimeAckStatus::Rejected,
                                bar_close_ts + 1,
                            ),
                        },
                        Stage5cPaperAckRecord {
                            total_sequence: 2,
                            ack: stage5ci_ack_with(
                                sl_request_id,
                                broker_core::HybridRuntimeAckStatus::Accepted,
                                bar_close_ts + 1,
                            ),
                        },
                    ],
                },
            )
            .unwrap();
            let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![stage5cj_stop_event(
                        3,
                        sl_request_id,
                        BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                        status,
                        bar_close_ts + 600,
                        bar_close_ts + 2,
                    )],
                },
            )
            .unwrap();
            assert!(broker_resolved
                .remaining_lifecycle_expectations()
                .is_empty());
        }
    }

    #[test]
    fn stage5cj_place_non_execution_terminal_finishes_without_position() {
        for status in ["canceled", "expired", "rejected"] {
            let (settled, request_id, bar_close_ts) =
                stage5cj_place_entry_settled(crate::BrokerNeutralOrderSide::Buy, 1.0);
            let resolved = resolve_stage5c_paper_intent_lifecycle(
                settled,
                Stage5cPaperIntentLifecycleInput {
                    ack_records: vec![stage5ci_ack_record(1, request_id)],
                },
            )
            .unwrap();
            let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![stage5cj_place_order_event(
                        2,
                        request_id,
                        status,
                        "buy",
                        1.0,
                        bar_close_ts + 2,
                    )],
                },
            )
            .unwrap();
            assert!(broker_resolved
                .remaining_lifecycle_expectations()
                .is_empty());
        }
    }

    #[test]
    fn stage5cj_place_filled_still_requires_position_confirmation() {
        let (settled, request_id, bar_close_ts) =
            stage5cj_place_entry_settled(crate::BrokerNeutralOrderSide::Buy, 1.0);
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![stage5cj_place_order_event(
                    2,
                    request_id,
                    "filled",
                    "buy",
                    1.0,
                    bar_close_ts + 2,
                )],
            },
        )
        .unwrap();
        assert_eq!(
            broker_resolved.remaining_lifecycle_expectations()[0].expected_event_kind,
            Stage5cPaperBrokerEventKind::Position
        );
    }

    #[test]
    fn stage5cj_partial_entry_position_reduction_blocks_before_callback() {
        let (settled, request_id, bar_close_ts) =
            stage5cj_place_entry_settled(crate::BrokerNeutralOrderSide::Buy, 3.0);
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        assert_eq!(
            resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![
                        stage5cj_place_order_event(
                            2,
                            request_id,
                            "filled",
                            "buy",
                            3.0,
                            bar_close_ts + 2,
                        ),
                        stage5cj_position_event(3, request_id, 1.0, bar_close_ts + 3),
                        stage5cj_position_event(4, request_id, 0.5, bar_close_ts + 4),
                    ]
                },
            )
            .expect_err("partial entry regression must be blocked before source callback")
            .reason(),
            Stage5cPaperBrokerLifecycleError::PositionRegression
        );
    }

    #[test]
    fn stage5cj_place_entry_rejects_wrong_side_position() {
        let (settled, request_id, bar_close_ts) =
            stage5cj_place_entry_settled(crate::BrokerNeutralOrderSide::Buy, 1.0);
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        assert_eq!(
            resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![
                        stage5cj_place_order_event(
                            2,
                            request_id,
                            "filled",
                            "buy",
                            1.0,
                            bar_close_ts + 2
                        ),
                        stage5cj_position_event(3, request_id, -1.0, bar_close_ts + 3),
                    ]
                },
            )
            .expect_err("place buy cannot settle into short broker position")
            .reason(),
            Stage5cPaperBrokerLifecycleError::PositionSideMismatch
        );

        let (settled, request_id, bar_close_ts) =
            stage5cj_place_entry_settled(crate::BrokerNeutralOrderSide::Sell, 1.0);
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        assert_eq!(
            resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![
                        stage5cj_place_order_event(
                            2,
                            request_id,
                            "filled",
                            "sell",
                            1.0,
                            bar_close_ts + 2
                        ),
                        stage5cj_position_event(3, request_id, 1.0, bar_close_ts + 3),
                    ]
                },
            )
            .expect_err("place sell cannot settle into long broker position")
            .reason(),
            Stage5cPaperBrokerLifecycleError::PositionSideMismatch
        );
    }

    #[test]
    fn stage5cj_partial_place_entry_keeps_expectation_and_target_closes() {
        let (settled, request_id, bar_close_ts) =
            stage5cj_place_entry_settled(crate::BrokerNeutralOrderSide::Buy, 3.0);
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        let partial = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![
                    stage5cj_place_order_event(
                        2,
                        request_id,
                        "filled",
                        "buy",
                        3.0,
                        bar_close_ts + 2,
                    ),
                    stage5cj_position_event(3, request_id, 1.0, bar_close_ts + 3),
                ],
            },
        )
        .unwrap();
        assert_eq!(
            partial.remaining_lifecycle_expectations()[0].expected_event_kind,
            Stage5cPaperBrokerEventKind::Position
        );

        let (settled, request_id, bar_close_ts) =
            stage5cj_place_entry_settled(crate::BrokerNeutralOrderSide::Buy, 3.0);
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        let complete = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![
                    stage5cj_place_order_event(
                        2,
                        request_id,
                        "filled",
                        "buy",
                        3.0,
                        bar_close_ts + 2,
                    ),
                    stage5cj_position_event(3, request_id, 3.0, bar_close_ts + 3),
                ],
            },
        )
        .unwrap();
        assert!(complete.remaining_lifecycle_expectations().is_empty());
    }

    #[test]
    fn stage5cj_place_and_market_entry_overfill_block_before_callback() {
        let (settled, request_id, bar_close_ts) =
            stage5cj_place_entry_settled(crate::BrokerNeutralOrderSide::Buy, 1.0);
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        assert_eq!(
            resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![
                        stage5cj_place_order_event(
                            2,
                            request_id,
                            "filled",
                            "buy",
                            1.0,
                            bar_close_ts + 2
                        ),
                        stage5cj_position_event(3, request_id, 2.0, bar_close_ts + 3),
                    ]
                },
            )
            .expect_err("place overfill must be blocked before source callback")
            .reason(),
            Stage5cPaperBrokerLifecycleError::PositionOverfill
        );

        let (settled, request_id, bar_close_ts) = stage5ci_entry_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        assert_eq!(
            resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![stage5cj_position_event(
                        2,
                        request_id,
                        2.0,
                        bar_close_ts + 2
                    )]
                },
            )
            .expect_err("market overfill must be blocked before source callback")
            .reason(),
            Stage5cPaperBrokerLifecycleError::PositionOverfill
        );
    }

    #[test]
    fn stage5cj_tp_cancel_rejects_wrong_role_and_cycle() {
        for (role, cycle, expected) in [
            (
                "ENTRY",
                "abc1230001",
                Stage5cPaperBrokerLifecycleError::AttributionRoleMismatch,
            ),
            (
                "TP",
                "deadbeef01",
                Stage5cPaperBrokerLifecycleError::AttributionCycleMismatch,
            ),
        ] {
            let (settled, request_id, bar_close_ts) = stage5cj_cleanup_cancel_settled();
            let resolved = resolve_stage5c_paper_intent_lifecycle(
                settled,
                Stage5cPaperIntentLifecycleInput {
                    ack_records: vec![Stage5cPaperAckRecord {
                        total_sequence: 1,
                        ack: broker_core::HybridRuntimeCommandAck {
                            broker_order_id: Some(BrokerOrderId::new("ORDER_TEST_0001")),
                            ..stage5ci_ack(request_id)
                        },
                    }],
                },
            )
            .unwrap();
            let mut event = stage5cj_order_event_with_role(
                2,
                request_id,
                BrokerOrderId::new("ORDER_TEST_0001"),
                "canceled",
                "sell",
                2230.0,
                role,
                bar_close_ts + 2,
            );
            if let Stage5cPaperBrokerEventPayload::Order(order) = &mut event.payload {
                order.attribution = Some(stage5cj_attribution_with_cycle(role, cycle));
            }
            assert_eq!(
                resolve_stage5c_paper_broker_lifecycle(
                    resolved,
                    Stage5cPaperBrokerLifecycleInput {
                        event_records: vec![event]
                    },
                )
                .expect_err("TP cancel must preserve original attribution")
                .reason(),
                expected
            );
        }
    }

    #[test]
    fn stage5cj_sl_delete_rejects_wrong_role_and_cycle() {
        for (role, cycle, expected) in [
            (
                "TP",
                "abc1230001",
                Stage5cPaperBrokerLifecycleError::AttributionRoleMismatch,
            ),
            (
                "SL",
                "deadbeef01",
                Stage5cPaperBrokerLifecycleError::AttributionCycleMismatch,
            ),
        ] {
            let (settled, request_id, bar_close_ts) = stage5cj_cleanup_delete_stop_settled();
            let resolved = resolve_stage5c_paper_intent_lifecycle(
                settled,
                Stage5cPaperIntentLifecycleInput {
                    ack_records: vec![Stage5cPaperAckRecord {
                        total_sequence: 1,
                        ack: broker_core::HybridRuntimeCommandAck {
                            broker_order_id: Some(BrokerOrderId::new("ORDER_TEST_ACK_0001")),
                            ..stage5ci_ack(request_id)
                        },
                    }],
                },
            )
            .unwrap();
            let mut event = stage5cj_stop_event(
                2,
                request_id,
                BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                "canceled",
                bar_close_ts + 600,
                bar_close_ts + 2,
            );
            if let Stage5cPaperBrokerEventPayload::StopOrder(stop) = &mut event.payload {
                stop.attribution = Some(stage5cj_attribution_with_cycle(role, cycle));
            }
            assert_eq!(
                resolve_stage5c_paper_broker_lifecycle(
                    resolved,
                    Stage5cPaperBrokerLifecycleInput {
                        event_records: vec![event]
                    },
                )
                .expect_err("SL delete must preserve original attribution")
                .reason(),
                expected
            );
        }
    }

    #[test]
    fn stage5cj_position_flat_preserves_generated_cleanup_intents_with_original_attribution() {
        let (settled, request_id, bar_close_ts) = stage5ci_exit_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        let Stage5cResolvedPaperIntentBatchStrategy {
            mut strategy,
            recovery_receipt,
            resolved_batch,
            ack_outcomes,
            settled_batch_history,
        } = resolved;
        let mut state = Strategy::state(&strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                current_owner,
                current_side,
                last_position_qty,
                tp_order_id,
                sl_stop_order_id,
                sl_exchange_order_id,
                ..
            } => {
                *active_cycle_id = Some("abc1230001".to_string());
                *current_owner = Some(crate::hybrid_intraday::Owner::MeanReversion);
                *current_side = Some(crate::hybrid_intraday::Side::Long);
                *last_position_qty = 1.0;
                *tp_order_id = Some(BrokerOrderId::new("TP_ORDER_TEST_0001"));
                *sl_stop_order_id = Some(BrokerStopOrderId::new("STOP_TEST_0001"));
                *sl_exchange_order_id = Some(BrokerOrderId::new("SL_EXCHANGE_TEST_0001"));
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(&mut strategy, state);
        let resolved = Stage5cResolvedPaperIntentBatchStrategy {
            strategy,
            recovery_receipt,
            resolved_batch,
            ack_outcomes,
            settled_batch_history,
        };
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![stage5cj_position_event(
                    2,
                    request_id,
                    0.0,
                    bar_close_ts + 2,
                )],
            },
        )
        .unwrap();
        let generated = broker_resolved
            .generated_intent_batch()
            .expect("flat position cleanup must be preserved as no-send generated batch");
        assert_eq!(generated.intent_count(), 3);
        let source_ts = bar_close_ts + 2;
        assert!(generated
            .request_ids()
            .contains(&crate::deterministic_request_id(
                "hybrid_imoexf",
                "ACC_TEST_0001",
                "IMOEXF",
                "cancel:TP_ORDER_TEST_0001",
                source_ts,
                1,
            )));
        assert!(generated
            .request_ids()
            .contains(&crate::deterministic_request_id(
                "hybrid_imoexf",
                "ACC_TEST_0001",
                "IMOEXF",
                "cancel:SL_EXCHANGE_TEST_0001",
                source_ts,
                1,
            )));
        assert!(generated
            .request_ids()
            .contains(&crate::deterministic_request_id(
                "hybrid_imoexf",
                "ACC_TEST_0001",
                "IMOEXF",
                "delete_stop_limit:STOP_TEST_0001",
                source_ts,
                6,
            )));
        assert!(!generated
            .request_ids()
            .contains(&crate::deterministic_request_id(
                "hybrid_imoexf",
                "ACC_TEST_0001",
                "IMOEXF",
                "cancel:TP_ORDER_TEST_0001",
                bar_close_ts,
                1,
            )));
        let roles: Vec<_> = generated
            .records
            .iter()
            .map(|record| {
                record
                    .expected_attribution
                    .as_ref()
                    .and_then(broker_core::HybridRuntimeAttribution::role)
            })
            .collect();
        assert_eq!(
            roles,
            vec![
                Some(broker_core::HybridRuntimeOrderRole::TakeProfit),
                Some(broker_core::HybridRuntimeOrderRole::StopLoss),
                Some(broker_core::HybridRuntimeOrderRole::StopLoss),
            ]
        );
        assert!(generated.records.iter().all(|record| record
            .expected_attribution
            .as_ref()
            .is_some_and(|attr| {
                attr.cycle_id() == "abc1230001"
                    && attr.owner() == Some(broker_core::HybridRuntimeOwner::MeanReversion)
            })));
    }

    #[test]
    fn stage5cj_merged_generated_batch_preserves_per_record_source_ts_and_final_fingerprint() {
        fn run_multi_callback_generated_case() -> (
            Stage5cBrokerLifecycleResolvedPaperStrategy,
            StrategyRequestId,
            StrategyRequestId,
            i64,
            i64,
        ) {
            let (settled, tp_request_id, sl_request_id, bar_close_ts) =
                stage5ci_protective_settled();
            let mut resolved = resolve_stage5c_paper_intent_lifecycle(
                settled,
                Stage5cPaperIntentLifecycleInput {
                    ack_records: vec![
                        Stage5cPaperAckRecord {
                            total_sequence: 1,
                            ack: broker_core::HybridRuntimeCommandAck {
                                broker_order_id: Some(BrokerOrderId::new("TP_ORDER_TEST_0001")),
                                ..stage5ci_ack_with(
                                    tp_request_id,
                                    broker_core::HybridRuntimeAckStatus::Accepted,
                                    bar_close_ts + 1,
                                )
                            },
                        },
                        Stage5cPaperAckRecord {
                            total_sequence: 2,
                            ack: broker_core::HybridRuntimeCommandAck {
                                broker_order_id: Some(BrokerOrderId::new("SL_EXCHANGE_TEST_0001")),
                                ..stage5ci_ack_with(
                                    sl_request_id,
                                    broker_core::HybridRuntimeAckStatus::Accepted,
                                    bar_close_ts + 1,
                                )
                            },
                        },
                    ],
                },
            )
            .unwrap();
            let mut state = Strategy::state(&resolved.strategy).clone();
            match &mut state {
                StrategyState::HybridIntradayRuntime {
                    active_cycle_id,
                    current_owner,
                    current_side,
                    last_position_qty,
                    ..
                } => {
                    *active_cycle_id = Some("abc1230001".to_string());
                    *current_owner = Some(crate::hybrid_intraday::Owner::MeanReversion);
                    *current_side = Some(crate::hybrid_intraday::Side::Long);
                    *last_position_qty = 1.0;
                }
                StrategyState::Idle => panic!("expected hybrid runtime state"),
            }
            Strategy::set_state(&mut resolved.strategy, state);
            let stop_ts = bar_close_ts + 2;
            let flat_ts = bar_close_ts + 5;
            let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![
                        stage5cj_order_event(
                            3,
                            tp_request_id,
                            BrokerOrderId::new("TP_ORDER_TEST_0001"),
                            "working",
                            bar_close_ts + 1,
                        ),
                        stage5cj_stop_event(
                            4,
                            sl_request_id,
                            BrokerOrderId::new("SL_EXCHANGE_TEST_0001"),
                            "triggered",
                            bar_close_ts + 600,
                            stop_ts,
                        ),
                        stage5cj_position_event(5, sl_request_id, 0.0, flat_ts),
                    ],
                },
            )
            .unwrap();
            (
                broker_resolved,
                tp_request_id,
                sl_request_id,
                stop_ts,
                flat_ts,
            )
        }

        let (broker_resolved, _, _, stop_ts, flat_ts) = run_multi_callback_generated_case();
        let generated = broker_resolved
            .generated_intent_batch()
            .expect("stop trigger and flat position must produce generated cleanup batch");
        assert_eq!(generated.intent_count(), 2);
        assert_eq!(
            generated.state_fingerprint(),
            broker_resolved.post_broker_lifecycle_state_fingerprint()
        );
        let source_ts_by_request: HashMap<_, _> = generated
            .record_source_event_ts_by_request()
            .into_iter()
            .collect();
        let cancel_tp_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "cancel:TP_ORDER_TEST_0001",
            stop_ts,
            1,
        );
        let cancel_sl_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "cancel:SL_EXCHANGE_TEST_0001",
            flat_ts,
            1,
        );
        assert_eq!(
            source_ts_by_request.get(&cancel_tp_request_id),
            Some(&stop_ts)
        );
        assert_eq!(
            source_ts_by_request.get(&cancel_sl_request_id),
            Some(&flat_ts)
        );
        let summary = broker_resolved.generated_intent_batch_summary().unwrap();
        assert_eq!(summary.min_source_event_ts, stop_ts);
        assert_eq!(summary.max_source_event_ts, flat_ts);

        let (mut early_broker_resolved, _, _, _, early_flat_ts) =
            run_multi_callback_generated_case();
        let early_generated_batch = early_broker_resolved
            .generated_intent_batch
            .take()
            .expect("generated batch must exist");
        let early_settled = Stage5cSettledPaperStrategy {
            strategy: early_broker_resolved.strategy,
            recovery_receipt: early_broker_resolved.recovery_receipt,
            batch: early_generated_batch,
            settled_batch_history: early_broker_resolved.settled_batch_history,
        };
        assert_eq!(
            resolve_stage5c_paper_intent_lifecycle(
                early_settled,
                Stage5cPaperIntentLifecycleInput {
                    ack_records: vec![
                        Stage5cPaperAckRecord {
                            total_sequence: 6,
                            ack: stage5ci_ack_with(
                                cancel_tp_request_id,
                                broker_core::HybridRuntimeAckStatus::Accepted,
                                stop_ts,
                            ),
                        },
                        Stage5cPaperAckRecord {
                            total_sequence: 7,
                            ack: stage5ci_ack_with(
                                cancel_sl_request_id,
                                broker_core::HybridRuntimeAckStatus::Accepted,
                                early_flat_ts - 1,
                            ),
                        },
                    ],
                },
            )
            .expect_err("second generated ACK before its own source timestamp must block")
            .reason(),
            Stage5cPaperIntentLifecycleError::AckTimestampBeforeIntentBar
        );

        let (mut ok_broker_resolved, _, _, _, ok_flat_ts) = run_multi_callback_generated_case();
        let ok_generated_batch = ok_broker_resolved
            .generated_intent_batch
            .take()
            .expect("generated batch must exist");
        let ok_settled = Stage5cSettledPaperStrategy {
            strategy: ok_broker_resolved.strategy,
            recovery_receipt: ok_broker_resolved.recovery_receipt,
            batch: ok_generated_batch,
            settled_batch_history: ok_broker_resolved.settled_batch_history,
        };
        let generated_ack_resolved = resolve_stage5c_paper_intent_lifecycle(
            ok_settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![
                    Stage5cPaperAckRecord {
                        total_sequence: 6,
                        ack: stage5ci_ack_with(
                            cancel_tp_request_id,
                            broker_core::HybridRuntimeAckStatus::Accepted,
                            stop_ts,
                        ),
                    },
                    Stage5cPaperAckRecord {
                        total_sequence: 7,
                        ack: stage5ci_ack_with(
                            cancel_sl_request_id,
                            broker_core::HybridRuntimeAckStatus::Accepted,
                            ok_flat_ts,
                        ),
                    },
                ],
            },
        )
        .expect("generated ACK lifecycle must use final fingerprint and per-record source ts");
        assert_eq!(generated_ack_resolved.ack_outcomes().len(), 2);
    }

    #[test]
    fn stage5cj_generated_executable_intents_require_final_pending_state() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (strategy, receipt) = recovered.into_parts();
        let admission = &receipt
            .warmup_receipt()
            .restore_receipt()
            .bootstrap_receipt()
            .admission;
        let source_ts = now.timestamp() + 600;
        let exit_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            source_ts,
            4,
        );
        let cleanup_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "cancel:TP_ORDER_TEST_0001",
            source_ts,
            1,
        );
        let exit_batch = Stage5cPaperIntentBatch {
            strategy_id: admission.strategy_id().to_string(),
            account_id: admission.account_id().clone(),
            instrument: admission.target_instrument().clone(),
            bar_close_ts: source_ts,
            state_fingerprint: stage5c_state_fingerprint(Strategy::state(&strategy)),
            request_ids: vec![exit_request_id],
            records: vec![Stage5cPaperIntentRecord {
                request_id: exit_request_id,
                source_event_ts: source_ts,
                intent_class: crate::BrokerNeutralHybridIntentClass::Exit,
                intent: stage5cg_market_intent(
                    crate::BrokerNeutralOrderSide::Sell,
                    crate::BrokerNeutralHybridIntentClass::Exit,
                ),
                expected_attribution: None,
            }],
            observation_only: false,
        };
        assert_eq!(
            stage5cj_verify_generated_batch_final_pending_consistency(
                Strategy::state(&strategy),
                &exit_batch,
            )
            .expect_err("executable generated exit without final pending state must block"),
            Stage5cIntentSettlementError::MissingPendingRequest
        );

        let cleanup_batch = Stage5cPaperIntentBatch {
            strategy_id: admission.strategy_id().to_string(),
            account_id: admission.account_id().clone(),
            instrument: admission.target_instrument().clone(),
            bar_close_ts: source_ts,
            state_fingerprint: stage5c_state_fingerprint(Strategy::state(&strategy)),
            request_ids: vec![cleanup_request_id],
            records: vec![Stage5cPaperIntentRecord {
                request_id: cleanup_request_id,
                source_event_ts: source_ts,
                intent_class: crate::BrokerNeutralHybridIntentClass::CancelCleanup,
                intent: crate::BrokerNeutralHybridIntent::Cancel {
                    order_id: BrokerOrderId::new("TP_ORDER_TEST_0001"),
                }
                .with_class(crate::BrokerNeutralHybridIntentClass::CancelCleanup)
                .with_symbol("IMOEXF"),
                expected_attribution: None,
            }],
            observation_only: false,
        };
        stage5cj_verify_generated_batch_final_pending_consistency(
            Strategy::state(&strategy),
            &cleanup_batch,
        )
        .expect("cleanup generated intents do not require final pending state");
    }

    fn stage5ck_clean_broker_resolved_at(
        ack_ts_utc: i64,
        position_ts_utc: i64,
    ) -> (Stage5cBrokerLifecycleResolvedPaperStrategy, i64) {
        let (settled, request_id, bar_close_ts) = stage5ci_exit_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![Stage5cPaperAckRecord {
                    total_sequence: 1,
                    ack: stage5ci_ack_with(
                        request_id,
                        broker_core::HybridRuntimeAckStatus::Accepted,
                        ack_ts_utc,
                    ),
                }],
            },
        )
        .unwrap();
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![stage5cj_position_event(2, request_id, 0.0, position_ts_utc)],
            },
        )
        .unwrap();
        (broker_resolved, bar_close_ts)
    }

    fn stage5ck_clean_broker_resolved() -> (Stage5cBrokerLifecycleResolvedPaperStrategy, i64) {
        let (settled, request_id, bar_close_ts) = stage5ci_exit_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![stage5cj_position_event(
                    2,
                    request_id,
                    0.0,
                    bar_close_ts + 2,
                )],
            },
        )
        .unwrap();
        (broker_resolved, bar_close_ts)
    }

    #[test]
    fn stage5ck_zero_intent_timer_invokes_only_paper_timer_callback() {
        let (broker_resolved, bar_close_ts) = stage5ck_clean_broker_resolved();
        let timer = resolve_stage5c_paper_timer(
            broker_resolved,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: (bar_close_ts + 10) * 1_000,
            },
        )
        .unwrap();
        assert_eq!(timer.generated_intent_count(), 0);
        assert!(!timer.intent_sink_attached());
        assert!(!timer.broker_transport_attached());
        assert!(!timer.redis_command_stream_attached());
    }

    #[test]
    fn stage5ck_timer_clock_is_bound_to_lifecycle_watermark() {
        let (broker_resolved, bar_close_ts) = stage5ck_clean_broker_resolved();
        assert_eq!(
            broker_resolved.lifecycle_watermark_ts_utc(),
            bar_close_ts + 2
        );
        let blocked = resolve_stage5c_paper_timer(
            broker_resolved,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: (bar_close_ts + 1) * 1_000,
            },
        )
        .expect_err("timer before latest PositionEvent must be blocked")
        .into_blocked()
        .expect("non-monotonic timer preserves broker-resolved type-state");
        assert_eq!(blocked.reason(), Stage5cPaperTimerError::NonMonotonicTimer);
        let broker_resolved = blocked.into_resolved();
        assert_eq!(
            broker_resolved.lifecycle_watermark_ts_utc(),
            bar_close_ts + 2
        );

        let timer = resolve_stage5c_paper_timer(
            broker_resolved,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: (bar_close_ts + 2) * 1_000,
            },
        )
        .expect("timer exactly at lifecycle watermark is allowed");
        assert_eq!(timer.generated_intent_count(), 0);

        let (broker_resolved, bar_close_ts) =
            stage5ck_clean_broker_resolved_at(bar_close_ts + 5, bar_close_ts + 5);
        assert_eq!(
            broker_resolved.lifecycle_watermark_ts_utc(),
            bar_close_ts + 5
        );
        assert_eq!(
            resolve_stage5c_paper_timer(
                broker_resolved,
                Stage5cPaperTimerInput {
                    now_ts_utc_ms: (bar_close_ts + 4) * 1_000,
                },
            )
            .expect_err("timer before latest ACK/event timestamp must be blocked")
            .reason(),
            Stage5cPaperTimerError::NonMonotonicTimer
        );

        let (broker_resolved, bar_close_ts) = stage5ck_clean_broker_resolved();
        let timer = resolve_stage5c_paper_timer(
            broker_resolved,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: (bar_close_ts + 10) * 1_000,
            },
        )
        .expect("timer later than lifecycle watermark is allowed");
        assert_eq!(timer.generated_intent_count(), 0);
    }

    #[test]
    fn stage5ck_blocks_unresolved_broker_lifecycle_and_generated_batch() {
        let (settled, tp_request_id, sl_request_id, bar_close_ts) = stage5ci_protective_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![
                    Stage5cPaperAckRecord {
                        total_sequence: 1,
                        ack: stage5ci_ack_with(
                            tp_request_id,
                            broker_core::HybridRuntimeAckStatus::Accepted,
                            bar_close_ts + 1,
                        ),
                    },
                    Stage5cPaperAckRecord {
                        total_sequence: 2,
                        ack: stage5ci_ack_with(
                            sl_request_id,
                            broker_core::HybridRuntimeAckStatus::Accepted,
                            bar_close_ts + 1,
                        ),
                    },
                ],
            },
        )
        .unwrap();
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![
                    stage5cj_order_event(
                        3,
                        tp_request_id,
                        BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                        "working",
                        bar_close_ts + 2,
                    ),
                    stage5cj_stop_event(
                        4,
                        sl_request_id,
                        BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                        "working",
                        bar_close_ts + 600,
                        bar_close_ts + 2,
                    ),
                ],
            },
        )
        .unwrap();
        assert_eq!(
            resolve_stage5c_paper_timer(
                broker_resolved,
                Stage5cPaperTimerInput {
                    now_ts_utc_ms: (bar_close_ts + 10) * 1_000,
                },
            )
            .expect_err("timer must wait for complete broker lifecycle")
            .reason(),
            Stage5cPaperTimerError::UnresolvedBrokerLifecycle
        );

        let (settled, request_id, bar_close_ts) = stage5ci_exit_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        let Stage5cResolvedPaperIntentBatchStrategy {
            mut strategy,
            recovery_receipt,
            resolved_batch,
            ack_outcomes,
            settled_batch_history,
        } = resolved;
        let mut state = Strategy::state(&strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                current_owner,
                current_side,
                last_position_qty,
                tp_order_id,
                ..
            } => {
                *active_cycle_id = Some("abc1230001".to_string());
                *current_owner = Some(crate::hybrid_intraday::Owner::MeanReversion);
                *current_side = Some(crate::hybrid_intraday::Side::Long);
                *last_position_qty = 1.0;
                *tp_order_id = Some(BrokerOrderId::new("TP_ORDER_TEST_0001"));
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(&mut strategy, state);
        let resolved = Stage5cResolvedPaperIntentBatchStrategy {
            strategy,
            recovery_receipt,
            resolved_batch,
            ack_outcomes,
            settled_batch_history,
        };
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![stage5cj_position_event(
                    2,
                    request_id,
                    0.0,
                    bar_close_ts + 2,
                )],
            },
        )
        .unwrap();
        assert!(broker_resolved.generated_intent_count() > 0);
        assert_eq!(
            resolve_stage5c_paper_timer(
                broker_resolved,
                Stage5cPaperTimerInput {
                    now_ts_utc_ms: (bar_close_ts + 10) * 1_000,
                },
            )
            .expect_err("timer must wait for generated batch lifecycle")
            .reason(),
            Stage5cPaperTimerError::UnresolvedGeneratedIntentBatch
        );
    }

    #[test]
    fn stage5cj_semantic_bar_cleanup_attribution_is_captured_before_wrapper_take() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (mut strategy, receipt) = recovered.into_parts();
        let close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let mut state = Strategy::state(&strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                current_owner,
                current_side,
                last_position_qty,
                tp_order_id,
                sl_stop_order_id,
                sl_exchange_order_id,
                sl_triggered_ts,
                mr_take_price,
                mr_stop_price,
                repair_deadline_ts,
                ..
            } => {
                *active_cycle_id = Some("abc1230001".to_string());
                *current_owner = Some(crate::hybrid_intraday::Owner::MeanReversion);
                *current_side = Some(crate::hybrid_intraday::Side::Long);
                *last_position_qty = 1.0;
                *tp_order_id = Some(BrokerOrderId::new("TP_ORDER_TEST_0001"));
                *sl_stop_order_id = Some(BrokerStopOrderId::new("STOP_TEST_0001"));
                *sl_exchange_order_id = Some(BrokerOrderId::new("SL_EXCHANGE_TEST_0001"));
                *sl_triggered_ts = Some(close_ts - 31);
                *mr_take_price = Some(2235.0);
                *mr_stop_price = Some(2210.0);
                *repair_deadline_ts = Some(close_ts - 1);
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(&mut strategy, state);
        let recovered = Stage5cPendingRecoveredPaperStrategy { strategy, receipt };
        let accepted = accept_stage5c_semantic_bar(semantic_input(close_ts)).unwrap();
        let semantic = apply_stage5c_semantic_bar_at(
            recovered,
            accepted,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 10, 30)
                .single()
                .unwrap(),
        )
        .unwrap();
        let settled = settle_stage5c_semantic_result(semantic).unwrap();
        let cleanup_records: Vec<_> = settled
            .intent_batch()
            .records
            .iter()
            .filter(|record| {
                record.intent_class == crate::BrokerNeutralHybridIntentClass::CancelCleanup
            })
            .collect();
        assert_eq!(cleanup_records.len(), 2);
        assert!(cleanup_records.iter().all(|record| record
            .expected_attribution
            .as_ref()
            .is_some_and(|attr| attr.cycle_id() == "abc1230001")));
        assert!(cleanup_records.iter().any(|record| record
            .expected_attribution
            .as_ref()
            .and_then(broker_core::HybridRuntimeAttribution::role)
            == Some(broker_core::HybridRuntimeOrderRole::TakeProfit)));
        assert_eq!(
            cleanup_records
                .iter()
                .filter(|record| {
                    record
                        .expected_attribution
                        .as_ref()
                        .and_then(broker_core::HybridRuntimeAttribution::role)
                        == Some(broker_core::HybridRuntimeOrderRole::StopLoss)
                })
                .count(),
            1
        );
    }

    #[test]
    fn stage5ck_partial_entry_cleanup_uses_pending_entry_attribution() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (mut strategy, receipt) = recovered.into_parts();
        let strategy_id = receipt
            .warmup_receipt()
            .restore_receipt()
            .bootstrap_receipt()
            .admission
            .strategy_id()
            .to_string();
        let mut state = Strategy::state(&strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                pending_entry_owner,
                pending_entry_side,
                pending_entry_cycle_id,
                pending_entry_request_id,
                ..
            } => {
                *pending_entry_owner = Some(crate::hybrid_intraday::Owner::MeanReversion);
                *pending_entry_side = Some(crate::hybrid_intraday::Side::Long);
                *pending_entry_cycle_id = Some("abc1230001".to_string());
                *pending_entry_request_id =
                    Some(StrategyRequestId::from(uuid::Uuid::from_u128(0x5c0ffee)));
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(&mut strategy, state);

        let ledger = stage5cj_cleanup_attribution_ledger(Strategy::state(&strategy), &strategy_id);
        let cancel_entry = crate::BrokerNeutralHybridIntent::Cancel {
            order_id: BrokerOrderId::new("ENTRY_WORKING_ORDER_TEST_0001"),
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::CancelCleanup);
        let attribution = stage5cj_expected_cleanup_attribution_from_ledger(&ledger, &cancel_entry)
            .expect("pending-entry cleanup cancel receives exact ENTRY attribution");
        assert_eq!(attribution.strategy_id(), strategy_id);
        assert_eq!(attribution.cycle_id(), "abc1230001");
        assert_eq!(
            attribution.role(),
            Some(broker_core::HybridRuntimeOrderRole::Entry)
        );
    }

    #[test]
    fn stage5cl_zero_timer_settlement_allows_controlled_continuation() {
        let (broker_resolved, bar_close_ts) = stage5ck_clean_broker_resolved();
        let timer_ts_utc = bar_close_ts + 10;
        let timer = resolve_stage5c_paper_timer(
            broker_resolved,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: timer_ts_utc * 1_000,
            },
        )
        .unwrap();
        let settlement = settle_stage5c_timer_result(timer);
        assert!(settlement.is_ready_for_continuation());
        assert!(!settlement.intent_sink_attached());
        assert!(!settlement.broker_transport_attached());
        assert!(!settlement.redis_command_stream_attached());
        assert_eq!(settlement.settled().intent_batch().intent_count(), 0);
        assert_eq!(
            settlement.settled().intent_batch().bar_close_ts(),
            timer_ts_utc
        );
        assert_eq!(settlement.settled().settled_batch_history().len(), 2);

        let next_close_ts = bar_close_ts + 600;
        let accepted = accept_stage5c_semantic_bar(semantic_input(next_close_ts)).unwrap();
        let advanced = advance_stage5c_timer_settlement_next_bar_at(
            settlement,
            accepted,
            Utc.timestamp_opt(next_close_ts + 30, 0).single().unwrap(),
        )
        .expect("zero timer settlement is a controlled continuation checkpoint");
        assert_eq!(advanced.intent_batch().bar_close_ts(), next_close_ts);
        assert_eq!(advanced.settled_batch_history().len(), 3);
    }

    #[test]
    fn stage5cl_nonzero_timer_batch_must_reenter_ack_lifecycle() {
        let (settled, _, bar_close_ts) = stage5ci_exit_settled();
        let Stage5cSettledPaperStrategy {
            strategy,
            recovery_receipt,
            batch,
            settled_batch_history,
        } = settled;
        let timer = Stage5cTimerResolvedPaperStrategy {
            strategy,
            recovery_receipt,
            resolved_batch_summary: stage5ch_batch_summary(&batch),
            timer_ts_utc_ms: (bar_close_ts + 10) * 1_000,
            generated_intent_batch: Some(batch),
            settled_batch_history,
        };

        let settlement = settle_stage5c_timer_result(timer);
        assert!(settlement.is_generated_intent_batch());
        assert_eq!(settlement.settled().intent_batch().intent_count(), 1);
        let generated_settled = settlement
            .into_generated_intent_batch()
            .expect("generated timer settlement exposes only the generated batch");
        assert_eq!(
            advance_stage5c_controlled_next_bar_at(
                generated_settled,
                accept_stage5c_semantic_bar(semantic_input(bar_close_ts + 600)).unwrap(),
                Utc.timestamp_opt(bar_close_ts + 630, 0).single().unwrap(),
            )
            .expect_err("nonzero timer batch must not skip ACK lifecycle")
            .reason(),
            Stage5cNextBarLoopError::UnresolvedIntentBatch
        );

        let (settled, request_id, bar_close_ts) = stage5ci_exit_settled();
        let Stage5cSettledPaperStrategy {
            strategy,
            recovery_receipt,
            batch,
            settled_batch_history,
        } = settled;
        let timer = Stage5cTimerResolvedPaperStrategy {
            strategy,
            recovery_receipt,
            resolved_batch_summary: stage5ch_batch_summary(&batch),
            timer_ts_utc_ms: (bar_close_ts + 10) * 1_000,
            generated_intent_batch: Some(batch),
            settled_batch_history,
        };
        let generated_settled = settle_stage5c_timer_result(timer)
            .into_generated_intent_batch()
            .expect("timer-generated batch reuses the Stage 5C-i ACK lifecycle");
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            generated_settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .expect("timer-generated batch reuses the Stage 5C-i ACK lifecycle");
        assert_eq!(resolved.resolved_batch_summary().intent_count, 1);
    }

    #[test]
    fn stage5cm_ready_checkpoint_can_continue_to_timer_or_bar_once() {
        let (broker_resolved, bar_close_ts) = stage5ck_clean_broker_resolved();
        let timer_ts_utc = bar_close_ts + 10;
        let first_timer = resolve_stage5c_paper_timer(
            broker_resolved,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: timer_ts_utc * 1_000,
            },
        )
        .unwrap();
        let ready = settle_stage5c_timer_result(first_timer);
        let second_timer = advance_stage5c_timer_settlement_timer(
            ready,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: (timer_ts_utc + 1) * 1_000,
            },
        )
        .expect("ready timer checkpoint may advance to one later timer");
        assert_eq!(second_timer.generated_intent_count(), 0);

        let (broker_resolved, bar_close_ts) = stage5ck_clean_broker_resolved();
        let timer_ts_utc = bar_close_ts + 10;
        let first_timer = resolve_stage5c_paper_timer(
            broker_resolved,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: timer_ts_utc * 1_000,
            },
        )
        .unwrap();
        let ready = settle_stage5c_timer_result(first_timer);
        let next_close_ts = bar_close_ts + 600;
        let advanced = advance_stage5c_timer_settlement_next_bar_at(
            ready,
            accept_stage5c_semantic_bar(semantic_input(next_close_ts)).unwrap(),
            Utc.timestamp_opt(next_close_ts + 30, 0).single().unwrap(),
        )
        .expect("ready timer checkpoint may advance to one later bar");
        assert_eq!(advanced.intent_batch().bar_close_ts(), next_close_ts);
    }

    fn stage5cm_ready_subsecond_checkpoint() -> (Stage5cTimerSettlement, i64, i64) {
        let (broker_resolved, bar_close_ts) = stage5ck_clean_broker_resolved();
        let checkpoint_ts_utc_ms = (bar_close_ts + 10) * 1_000 + 900;
        let timer = resolve_stage5c_paper_timer(
            broker_resolved,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: checkpoint_ts_utc_ms,
            },
        )
        .unwrap();
        let settlement = settle_stage5c_timer_result(timer);
        assert!(settlement.is_ready_for_continuation());
        assert_eq!(
            settlement.checkpoint_ts_utc_ms(),
            Some(checkpoint_ts_utc_ms)
        );
        (settlement, bar_close_ts, checkpoint_ts_utc_ms)
    }

    #[test]
    fn stage5cm_timer_before_exact_millisecond_checkpoint_is_blocked() {
        let (settlement, _, checkpoint_ts_utc_ms) = stage5cm_ready_subsecond_checkpoint();
        let blocked = advance_stage5c_timer_settlement_timer(
            settlement,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: checkpoint_ts_utc_ms - 400,
            },
        )
        .expect_err("timer before exact millisecond checkpoint must be blocked");
        assert_eq!(
            blocked.reason(),
            Stage5cTimerContinuationError::NonMonotonicTimer
        );
        assert_eq!(
            blocked
                .into_blocked()
                .unwrap()
                .settlement()
                .checkpoint_ts_utc_ms(),
            Some(checkpoint_ts_utc_ms)
        );
    }

    #[test]
    fn stage5cm_timer_equal_to_exact_checkpoint_is_blocked() {
        let (settlement, _, checkpoint_ts_utc_ms) = stage5cm_ready_subsecond_checkpoint();
        let blocked = advance_stage5c_timer_settlement_timer(
            settlement,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: checkpoint_ts_utc_ms,
            },
        )
        .expect_err("timer equal to exact millisecond checkpoint must be blocked");
        assert_eq!(
            blocked.reason(),
            Stage5cTimerContinuationError::NonMonotonicTimer
        );
    }

    #[test]
    fn stage5cm_timer_one_millisecond_after_checkpoint_is_accepted() {
        let (settlement, _, checkpoint_ts_utc_ms) = stage5cm_ready_subsecond_checkpoint();
        let advanced = advance_stage5c_timer_settlement_timer(
            settlement,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: checkpoint_ts_utc_ms + 1,
            },
        )
        .expect("timer one millisecond after exact checkpoint is monotonic");
        assert_eq!(advanced.generated_intent_count(), 0);
        assert_eq!(advanced.timer_ts_utc_ms(), checkpoint_ts_utc_ms + 1);
    }

    #[test]
    fn stage5cm_blocked_subsecond_timer_preserves_settlement() {
        let (settlement, _, checkpoint_ts_utc_ms) = stage5cm_ready_subsecond_checkpoint();
        let blocked = advance_stage5c_timer_settlement_timer(
            settlement,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: checkpoint_ts_utc_ms - 400,
            },
        )
        .expect_err("subsecond nonmonotonic timer is recoverable");
        let blocked = blocked.into_blocked().unwrap();
        assert!(blocked.settlement().is_ready_for_continuation());
        assert_eq!(
            blocked.settlement().checkpoint_ts_utc_ms(),
            Some(checkpoint_ts_utc_ms)
        );
        assert_eq!(
            blocked.settlement().settled().intent_batch().intent_count(),
            0
        );
    }

    #[test]
    fn stage5cm_nonmonotonic_next_bar_preserves_ready_settlement() {
        let (settlement, bar_close_ts, checkpoint_ts_utc_ms) =
            stage5cm_ready_subsecond_checkpoint();
        let previous_fingerprint = settlement
            .settled()
            .intent_batch()
            .state_fingerprint()
            .to_string();
        let blocked = advance_stage5c_timer_settlement_next_bar_at(
            settlement,
            accept_stage5c_semantic_bar(semantic_input(bar_close_ts)).unwrap(),
            Utc.timestamp_opt(bar_close_ts + 30, 0).single().unwrap(),
        )
        .expect_err("nonmonotonic next bar must preserve ready settlement");
        assert_eq!(
            blocked.reason(),
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::NonMonotonicBar)
        );
        let blocked = blocked.into_blocked().unwrap();
        assert!(blocked.settlement().is_ready_for_continuation());
        assert_eq!(
            blocked.settlement().checkpoint_ts_utc_ms(),
            Some(checkpoint_ts_utc_ms)
        );
        assert_eq!(
            blocked
                .settlement()
                .settled()
                .intent_batch()
                .state_fingerprint(),
            previous_fingerprint
        );
    }

    #[test]
    fn stage5cm_expired_next_bar_preserves_ready_settlement() {
        let (settlement, bar_close_ts, checkpoint_ts_utc_ms) =
            stage5cm_ready_subsecond_checkpoint();
        let expires_at = settlement
            .settled()
            .recovery_receipt()
            .warmup_receipt()
            .restore_receipt()
            .bootstrap_receipt()
            .expires_at();
        let blocked = advance_stage5c_timer_settlement_next_bar_at(
            settlement,
            accept_stage5c_semantic_bar(semantic_input(bar_close_ts + 600)).unwrap(),
            expires_at + chrono::Duration::milliseconds(1),
        )
        .expect_err("expired next bar preflight must preserve ready settlement");
        assert_eq!(
            blocked.reason(),
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::Semantic(
                Stage5cSemanticBarError::BrokerTruthExpired,
            ))
        );
        let blocked = blocked.into_blocked().unwrap();
        assert!(blocked.settlement().is_ready_for_continuation());
        assert_eq!(
            blocked.settlement().checkpoint_ts_utc_ms(),
            Some(checkpoint_ts_utc_ms)
        );
    }

    #[test]
    fn stage5cm_blocked_next_bar_does_not_invoke_callback() {
        let (settlement, bar_close_ts, _) = stage5cm_ready_subsecond_checkpoint();
        let previous_fingerprint = settlement
            .settled()
            .intent_batch()
            .state_fingerprint()
            .to_string();
        let blocked = advance_stage5c_timer_settlement_next_bar_at(
            settlement,
            accept_stage5c_semantic_bar(semantic_input(bar_close_ts)).unwrap(),
            Utc.timestamp_opt(bar_close_ts + 30, 0).single().unwrap(),
        )
        .expect_err("blocked next bar should stop before semantic callback");
        let blocked_settlement = blocked.into_blocked().unwrap().into_settlement();
        assert_eq!(
            blocked_settlement
                .settled()
                .intent_batch()
                .state_fingerprint(),
            previous_fingerprint
        );
        assert_eq!(
            blocked_settlement.settled().intent_batch().intent_count(),
            0
        );
    }

    #[test]
    fn stage5cm_blocked_next_bar_allows_later_timer_retry() {
        let (settlement, bar_close_ts, checkpoint_ts_utc_ms) =
            stage5cm_ready_subsecond_checkpoint();
        let blocked = advance_stage5c_timer_settlement_next_bar_at(
            settlement,
            accept_stage5c_semantic_bar(semantic_input(bar_close_ts)).unwrap(),
            Utc.timestamp_opt(bar_close_ts + 30, 0).single().unwrap(),
        )
        .expect_err("recoverable next-bar block should return settlement for retry");
        let retry_settlement = blocked.into_blocked().unwrap().into_settlement();
        let retry = advance_stage5c_timer_settlement_timer(
            retry_settlement,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: checkpoint_ts_utc_ms + 1,
            },
        )
        .expect("ready settlement returned from blocked next-bar may continue via timer");
        assert_eq!(retry.generated_intent_count(), 0);
        assert_eq!(retry.timer_ts_utc_ms(), checkpoint_ts_utc_ms + 1);
    }

    #[test]
    fn stage5cm_generated_timer_batch_blocks_continuation_until_lifecycle() {
        let (settled, _, bar_close_ts) = stage5ci_exit_settled();
        let Stage5cSettledPaperStrategy {
            strategy,
            recovery_receipt,
            batch,
            settled_batch_history,
        } = settled;
        let timer = Stage5cTimerResolvedPaperStrategy {
            strategy,
            recovery_receipt,
            resolved_batch_summary: stage5ch_batch_summary(&batch),
            timer_ts_utc_ms: (bar_close_ts + 10) * 1_000,
            generated_intent_batch: Some(batch),
            settled_batch_history,
        };
        let generated = settle_stage5c_timer_result(timer);
        assert_eq!(
            advance_stage5c_timer_settlement_next_bar(
                generated,
                accept_stage5c_semantic_bar(semantic_input(bar_close_ts + 600)).unwrap(),
            )
            .expect_err("generated timer batch cannot advance directly to next bar")
            .reason(),
            Stage5cTimerContinuationError::GeneratedIntentBatchRequiresLifecycle
        );

        let (settled, _, bar_close_ts) = stage5ci_exit_settled();
        let Stage5cSettledPaperStrategy {
            strategy,
            recovery_receipt,
            batch,
            settled_batch_history,
        } = settled;
        let timer = Stage5cTimerResolvedPaperStrategy {
            strategy,
            recovery_receipt,
            resolved_batch_summary: stage5ch_batch_summary(&batch),
            timer_ts_utc_ms: (bar_close_ts + 10) * 1_000,
            generated_intent_batch: Some(batch),
            settled_batch_history,
        };
        let generated = settle_stage5c_timer_result(timer);
        let blocked = advance_stage5c_timer_settlement_timer(
            generated,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: (bar_close_ts + 11) * 1_000,
            },
        )
        .expect_err("generated timer batch cannot advance directly to another timer");
        assert_eq!(
            blocked.reason(),
            Stage5cTimerContinuationError::GeneratedIntentBatchRequiresLifecycle
        );
        assert!(blocked
            .into_blocked()
            .expect("blocked generated batch preserves settlement")
            .settlement()
            .is_generated_intent_batch());
    }

    // STAGE5G-D-R1A-AUTHORITY-TESTS-BEGIN: deterministic-bar-continuation-authority-v1
    fn stage5gd_r1a_ready_at(checkpoint_ts_utc_ms: i64) -> (Stage5cTimerSettlement, i64) {
        let (broker_resolved, bar_close_ts) = stage5ck_clean_broker_resolved();
        let timer = resolve_stage5c_paper_timer(
            broker_resolved,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: checkpoint_ts_utc_ms,
            },
        )
        .expect("R1-a fixture timer is admitted");
        (settle_stage5c_timer_result(timer), bar_close_ts)
    }

    #[test]
    fn stage5gd_r1a_reversed_bar_blocks_before_callback_and_preserves_settlement() {
        let bar_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let next_bar_close_ts = bar_close_ts + 600;
        let timer_checkpoint = (next_bar_close_ts + 300) * 1_000;
        let (settlement, _) = stage5gd_r1a_ready_at(timer_checkpoint);
        let before = settlement
            .settled()
            .intent_batch()
            .state_fingerprint()
            .to_string();
        let blocked = advance_stage5c_timer_settlement_next_bar_at_checkpoint(
            settlement,
            accept_stage5c_semantic_bar(semantic_input(next_bar_close_ts)).unwrap(),
            next_bar_close_ts * 1_000,
            timer_checkpoint,
        )
        .expect_err("bar preceding the timer checkpoint must block before callback");
        assert_eq!(
            blocked.reason(),
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::NonMonotonicBar)
        );
        let preserved = blocked.into_blocked().unwrap().into_settlement();
        assert_eq!(preserved.checkpoint_ts_utc_ms(), Some(timer_checkpoint));
        assert_eq!(
            preserved.settled().intent_batch().state_fingerprint(),
            before
        );
        assert_eq!(preserved.settled().settled_batch_history().len(), 2);
    }

    #[test]
    fn stage5gd_r1a_equal_bar_and_timer_checkpoint_blocks_before_callback() {
        let bar_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let next_bar_close_ts = bar_close_ts + 600;
        let checkpoint = next_bar_close_ts * 1_000;
        let (settlement, _) = stage5gd_r1a_ready_at(checkpoint);
        let blocked = advance_stage5c_timer_settlement_next_bar_at_checkpoint(
            settlement,
            accept_stage5c_semantic_bar(semantic_input(next_bar_close_ts)).unwrap(),
            checkpoint,
            checkpoint,
        )
        .expect_err("equal bar/timer checkpoint must block before callback");
        assert_eq!(
            blocked.reason(),
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::NonMonotonicBar)
        );
        assert_eq!(
            blocked
                .into_blocked()
                .unwrap()
                .settlement()
                .checkpoint_ts_utc_ms(),
            Some(checkpoint)
        );
    }

    #[test]
    fn stage5gd_r1a_later_bar_invokes_one_existing_stage5c_callback() {
        let (settlement, bar_close_ts) = stage5gd_r1a_ready_at(
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 10, 10)
                .single()
                .unwrap()
                .timestamp_millis(),
        );
        let before_history = settlement.settled().settled_batch_history().len();
        let next_bar_close_ts = bar_close_ts + 600;
        let advanced = advance_stage5c_timer_settlement_next_bar_at_checkpoint(
            settlement,
            accept_stage5c_semantic_bar(semantic_input(next_bar_close_ts)).unwrap(),
            next_bar_close_ts * 1_000,
            (bar_close_ts + 10) * 1_000,
        )
        .expect("later bar is admitted exactly once");
        assert_eq!(advanced.intent_batch().bar_close_ts(), next_bar_close_ts);
        assert_eq!(advanced.settled_batch_history().len(), before_history + 1);
    }

    #[test]
    fn stage5gd_r1a_explicit_clock_is_reproducible_and_process_clock_independent() {
        fn run_once() -> (String, Vec<Stage5cPaperIntentBatchSummary>) {
            let checkpoint = Utc
                .with_ymd_and_hms(2026, 7, 13, 9, 10, 10)
                .single()
                .unwrap()
                .timestamp_millis();
            let (settlement, bar_close_ts) = stage5gd_r1a_ready_at(checkpoint);
            let next_bar_close_ts = bar_close_ts + 600;
            let advanced = advance_stage5c_timer_settlement_next_bar_at_checkpoint(
                settlement,
                accept_stage5c_semantic_bar(semantic_input(next_bar_close_ts)).unwrap(),
                next_bar_close_ts * 1_000,
                checkpoint,
            )
            .unwrap();
            (
                advanced.intent_batch().state_fingerprint().to_string(),
                advanced.settled_batch_history().to_vec(),
            )
        }
        assert_eq!(run_once(), run_once());
    }

    #[test]
    fn stage5gd_r1a_explicit_now_after_expiry_is_retryable() {
        let checkpoint = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 10)
            .single()
            .unwrap()
            .timestamp_millis();
        let (settlement, bar_close_ts) = stage5gd_r1a_ready_at(checkpoint);
        let expires_at = settlement
            .settled()
            .recovery_receipt()
            .warmup_receipt()
            .restore_receipt()
            .bootstrap_receipt()
            .expires_at();
        let blocked = advance_stage5c_timer_settlement_next_bar_at_checkpoint(
            settlement,
            accept_stage5c_semantic_bar(semantic_input(bar_close_ts + 600)).unwrap(),
            (expires_at + chrono::Duration::milliseconds(1)).timestamp_millis(),
            checkpoint,
        )
        .expect_err("explicit event time after recovery expiry must block");
        assert_eq!(
            blocked.reason(),
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::Semantic(
                Stage5cSemanticBarError::BrokerTruthExpired,
            ))
        );
        assert!(blocked.into_blocked().is_some());
    }

    #[test]
    fn stage5gd_r1a_bar_checkpoint_overflow_is_retryable_before_callback() {
        let checkpoint = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 10)
            .single()
            .unwrap()
            .timestamp_millis();
        let (settlement, bar_close_ts) = stage5gd_r1a_ready_at(checkpoint);
        let mut accepted = accept_stage5c_semantic_bar(semantic_input(bar_close_ts + 600)).unwrap();
        accepted.bar.close_time_utc = i64::MAX;
        let blocked = advance_stage5c_timer_settlement_next_bar_at_checkpoint(
            settlement,
            accepted,
            checkpoint + 1,
            checkpoint,
        )
        .expect_err("checked bar checkpoint overflow must block before callback");
        assert_eq!(
            blocked.reason(),
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::Semantic(
                Stage5cSemanticBarError::InvalidTimestamp,
            ))
        );
        assert!(blocked.into_blocked().is_some());
    }

    #[test]
    fn stage5gd_r1a_generated_bar_intents_remain_in_stage5c_settled_batch() {
        let checkpoint = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 10)
            .single()
            .unwrap()
            .timestamp_millis();
        let (mut settlement, bar_close_ts) = stage5gd_r1a_ready_at(checkpoint);
        let next_bar_close_ts = bar_close_ts + 600;
        let Stage5cTimerSettlementKind::ReadyForContinuation { settled, .. } =
            &mut settlement.inner
        else {
            panic!("R1-a fixture must be ready");
        };
        let mut state = Strategy::state(&settled.strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                current_owner,
                current_side,
                last_position_qty,
                tp_order_id,
                sl_stop_order_id,
                sl_exchange_order_id,
                sl_triggered_ts,
                mr_take_price,
                mr_stop_price,
                repair_deadline_ts,
                ..
            } => {
                *active_cycle_id = Some("abc1230001".to_string());
                *current_owner = Some(crate::hybrid_intraday::Owner::MeanReversion);
                *current_side = Some(crate::hybrid_intraday::Side::Long);
                *last_position_qty = 1.0;
                *tp_order_id = Some(BrokerOrderId::new("TP_ORDER_TEST_0001"));
                *sl_stop_order_id = Some(BrokerStopOrderId::new("STOP_TEST_0001"));
                *sl_exchange_order_id = Some(BrokerOrderId::new("SL_EXCHANGE_TEST_0001"));
                *sl_triggered_ts = Some(next_bar_close_ts - 31);
                *mr_take_price = Some(2235.0);
                *mr_stop_price = Some(2210.0);
                *repair_deadline_ts = Some(next_bar_close_ts - 1);
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(&mut settled.strategy, state);
        let advanced = advance_stage5c_timer_settlement_next_bar_at_checkpoint(
            settlement,
            accept_stage5c_semantic_bar(semantic_input(next_bar_close_ts)).unwrap(),
            next_bar_close_ts * 1_000,
            checkpoint,
        )
        .expect("existing Stage 5C callback output remains settled");
        assert!(advanced.intent_batch().intent_count() > 0);
        assert_eq!(advanced.intent_batch().bar_close_ts(), next_bar_close_ts);
    }
    // STAGE5G-D-R1A-AUTHORITY-TESTS-END: deterministic-bar-continuation-authority-v1

    // STAGE5G-D-R1A-R1-AUTHORITY-TESTS-BEGIN: complete-precallback-transactional-admission-v1
    #[derive(Debug, PartialEq)]
    struct Stage5gdR1aR1SettlementSnapshot {
        checkpoint_ts_utc_ms: Option<i64>,
        state_fingerprint: String,
        settled_batch_history: Vec<Stage5cPaperIntentBatchSummary>,
        intent_count: usize,
        recovered_ts: DateTime<Utc>,
        replayed_events: usize,
        duplicate_events: usize,
        last_history_ts: i64,
        processed_history_bars: usize,
        input_history_bars: usize,
        strategy_id: String,
        account_id: String,
        target_instrument: InstrumentId,
        expires_at: DateTime<Utc>,
    }

    fn stage5gd_r1a_r1_snapshot(
        settlement: &Stage5cTimerSettlement,
    ) -> Stage5gdR1aR1SettlementSnapshot {
        let settled = settlement.settled();
        let recovery = settled.recovery_receipt();
        let warmup = recovery.warmup_receipt();
        let admission = &warmup.restore_receipt().bootstrap_receipt().admission;
        Stage5gdR1aR1SettlementSnapshot {
            checkpoint_ts_utc_ms: settlement.checkpoint_ts_utc_ms(),
            state_fingerprint: settled.intent_batch().state_fingerprint().to_string(),
            settled_batch_history: settled.settled_batch_history().to_vec(),
            intent_count: settled.intent_batch().intent_count(),
            recovered_ts: recovery.recovered_ts(),
            replayed_events: recovery.replayed_events(),
            duplicate_events: recovery.duplicate_events(),
            last_history_ts: warmup.last_history_ts(),
            processed_history_bars: warmup.processed_bars(),
            input_history_bars: warmup.input_bars(),
            strategy_id: admission.strategy_id().to_string(),
            account_id: format!("{:?}", admission.account_id()),
            target_instrument: admission.target_instrument().clone(),
            expires_at: admission.expires_at(),
        }
    }

    fn stage5gd_r1a_r1_assert_exact_block(
        blocked: Stage5cTimerContinuationFailure,
        expected_reason: Stage5cTimerContinuationError,
        before: Stage5gdR1aR1SettlementSnapshot,
    ) {
        assert_eq!(blocked.reason(), expected_reason);
        let preserved = blocked
            .into_blocked()
            .expect("pre-callback admission failure must remain retryable")
            .into_settlement();
        assert_eq!(stage5gd_r1a_r1_snapshot(&preserved), before);
        assert_eq!(stage5gd_r1a_r1_delegate_count(), 0);
    }

    fn stage5gd_r1a_r1_ready_fixture() -> (Stage5cTimerSettlement, i64, i64) {
        let checkpoint = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 10)
            .single()
            .unwrap()
            .timestamp_millis();
        let (settlement, bar_close_ts) = stage5gd_r1a_ready_at(checkpoint);
        (settlement, bar_close_ts, checkpoint)
    }

    #[test]
    fn stage5gd_r1a_r1_future_bar_is_retryable_and_exactly_preserved() {
        stage5gd_r1a_r1_reset_delegate_count();
        let (settlement, bar_close_ts, checkpoint) = stage5gd_r1a_r1_ready_fixture();
        let before = stage5gd_r1a_r1_snapshot(&settlement);
        let next_bar_close_ts = bar_close_ts + 600;
        let blocked = advance_stage5c_timer_settlement_next_bar_transactional_at_checkpoint(
            settlement,
            accept_stage5c_semantic_bar(semantic_input(next_bar_close_ts)).unwrap(),
            next_bar_close_ts * 1_000 - 1,
            checkpoint,
        )
        .expect_err("evaluation before bar close must be retryable");
        stage5gd_r1a_r1_assert_exact_block(
            blocked,
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::Semantic(
                Stage5cSemanticBarError::FutureBar,
            )),
            before,
        );
    }

    #[test]
    fn stage5gd_r1a_r1_wrong_instrument_is_retryable_and_exactly_preserved() {
        stage5gd_r1a_r1_reset_delegate_count();
        let (settlement, bar_close_ts, checkpoint) = stage5gd_r1a_r1_ready_fixture();
        let before = stage5gd_r1a_r1_snapshot(&settlement);
        let next_bar_close_ts = bar_close_ts + 600;
        let mut accepted = accept_stage5c_semantic_bar(semantic_input(next_bar_close_ts)).unwrap();
        accepted.bar.instrument.symbol = "OTHER_TEST_FUT".to_string();
        let blocked = advance_stage5c_timer_settlement_next_bar_transactional_at_checkpoint(
            settlement,
            accepted,
            next_bar_close_ts * 1_000,
            checkpoint,
        )
        .expect_err("wrong instrument must be retryable");
        stage5gd_r1a_r1_assert_exact_block(
            blocked,
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::Semantic(
                Stage5cSemanticBarError::InstrumentMismatch,
            )),
            before,
        );
    }

    #[test]
    fn stage5gd_r1a_r1_wrong_tick_is_retryable_and_exactly_preserved() {
        stage5gd_r1a_r1_reset_delegate_count();
        let (settlement, bar_close_ts, checkpoint) = stage5gd_r1a_r1_ready_fixture();
        let before = stage5gd_r1a_r1_snapshot(&settlement);
        let next_bar_close_ts = bar_close_ts + 600;
        let mut accepted = accept_stage5c_semantic_bar(semantic_input(next_bar_close_ts)).unwrap();
        accepted.tick_size = 1.0;
        let blocked = advance_stage5c_timer_settlement_next_bar_transactional_at_checkpoint(
            settlement,
            accepted,
            next_bar_close_ts * 1_000,
            checkpoint,
        )
        .expect_err("wrong tick size must be retryable");
        stage5gd_r1a_r1_assert_exact_block(
            blocked,
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::Semantic(
                Stage5cSemanticBarError::TickSizeMismatch,
            )),
            before,
        );
    }

    #[test]
    fn stage5gd_r1a_r1_stale_bar_is_retryable_and_exactly_preserved() {
        stage5gd_r1a_r1_reset_delegate_count();
        let (settlement, bar_close_ts, _) = stage5gd_r1a_r1_ready_fixture();
        let before = stage5gd_r1a_r1_snapshot(&settlement);
        let accepted = accept_stage5c_semantic_bar(semantic_input(bar_close_ts)).unwrap();
        let blocked = advance_stage5c_timer_settlement_next_bar_transactional_at_checkpoint(
            settlement,
            accepted,
            bar_close_ts * 1_000,
            (bar_close_ts - 1) * 1_000,
        )
        .expect_err("bar stale against settled batch must be retryable");
        stage5gd_r1a_r1_assert_exact_block(
            blocked,
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::NonMonotonicBar),
            before,
        );
    }

    #[test]
    fn stage5gd_r1a_r1_history_stale_bar_preserves_recovery_identity() {
        stage5gd_r1a_r1_reset_delegate_count();
        let (settlement, _, _) = stage5gd_r1a_r1_ready_fixture();
        let history_ts = settlement
            .settled()
            .recovery_receipt()
            .warmup_receipt()
            .last_history_ts();
        let before = stage5gd_r1a_r1_snapshot(&settlement);
        let accepted = accept_stage5c_semantic_bar(semantic_input(history_ts)).unwrap();
        let blocked = advance_stage5c_timer_settlement_next_bar_transactional_at_checkpoint(
            settlement,
            accepted,
            history_ts * 1_000,
            (history_ts - 1) * 1_000,
        )
        .expect_err("bar stale against warmup history must be retryable");
        stage5gd_r1a_r1_assert_exact_block(
            blocked,
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::Semantic(
                Stage5cSemanticBarError::StaleOrDuplicateBar,
            )),
            before,
        );
    }

    #[test]
    fn stage5gd_r1a_r1_unresolved_batch_is_retryable_and_exactly_preserved() {
        stage5gd_r1a_r1_reset_delegate_count();
        let (settled, _, bar_close_ts) = stage5ci_exit_settled();
        let settlement = Stage5cTimerSettlement::generated_intent_batch(settled);
        let before = stage5gd_r1a_r1_snapshot(&settlement);
        let blocked = advance_stage5c_timer_settlement_next_bar_transactional_at_checkpoint(
            settlement,
            accept_stage5c_semantic_bar(semantic_input(bar_close_ts + 600)).unwrap(),
            (bar_close_ts + 600) * 1_000,
            (bar_close_ts + 10) * 1_000,
        )
        .expect_err("unresolved generated batch must remain retryable");
        stage5gd_r1a_r1_assert_exact_block(
            blocked,
            Stage5cTimerContinuationError::GeneratedIntentBatchRequiresLifecycle,
            before,
        );
    }

    #[test]
    fn stage5gd_r1a_r1_valid_bar_delegates_exactly_once_deterministically() {
        fn run_once() -> (String, Vec<Stage5cPaperIntentBatchSummary>) {
            stage5gd_r1a_r1_reset_delegate_count();
            let (settlement, bar_close_ts, checkpoint) = stage5gd_r1a_r1_ready_fixture();
            let next_bar_close_ts = bar_close_ts + 600;
            let advanced = advance_stage5c_timer_settlement_next_bar_transactional_at_checkpoint(
                settlement,
                accept_stage5c_semantic_bar(semantic_input(next_bar_close_ts)).unwrap(),
                next_bar_close_ts * 1_000,
                checkpoint,
            )
            .expect("complete preflight permits one existing callback path");
            assert_eq!(stage5gd_r1a_r1_delegate_count(), 1);
            (
                advanced.intent_batch().state_fingerprint().to_string(),
                advanced.settled_batch_history().to_vec(),
            )
        }
        assert_eq!(run_once(), run_once());
    }
    // STAGE5G-D-R1A-R1-AUTHORITY-TESTS-END: complete-precallback-transactional-admission-v1

    #[test]
    fn stage5cn_settle_is_bounded_no_send_step() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (strategy, recovery_receipt) = recovered.into_parts();
        let first_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let semantic_state =
            Stage5cPaperLoopState::SemanticResult(Box::new(Stage5cSemanticBarResult {
                strategy,
                recovery_receipt,
                bar_close_ts: first_close_ts,
                origin: broker_core::HybridRuntimeBarOrigin::Live,
                execution_eligible: true,
                intents: Vec::new(),
                expected_attribution_by_request: HashMap::new(),
            }));

        let settled_state = advance_stage5c_paper_loop_once(
            semantic_state,
            Stage5cPaperLoopEvent::SettleSemanticResult,
        )
        .expect("bounded loop settles the captured semantic result explicitly");
        assert_eq!(settled_state.kind(), Stage5cPaperLoopStateKind::Settled);
        assert!(!settled_state.intent_sink_attached());
        assert!(!settled_state.broker_transport_attached());
        assert!(!settled_state.redis_command_stream_attached());
    }

    #[test]
    fn stage5cn_invalid_transition_preserves_input_state() {
        let recovered = empty_recovered(Utc::now());
        let failure = advance_stage5c_paper_loop_once(
            Stage5cPaperLoopState::PendingRecovered(Box::new(recovered)),
            Stage5cPaperLoopEvent::Timer(Stage5cPaperTimerInput {
                now_ts_utc_ms: Utc::now().timestamp_millis(),
            }),
        )
        .expect_err("timer is not a valid first paper-loop event");
        assert_eq!(
            failure.reason(),
            Stage5cPaperLoopError::InvalidTransition {
                state: Stage5cPaperLoopStateKind::PendingRecovered,
                event: Stage5cPaperLoopEventKind::Timer,
            }
        );
        assert_eq!(
            failure.preserved_state().map(Stage5cPaperLoopState::kind),
            Some(Stage5cPaperLoopStateKind::PendingRecovered)
        );
    }

    #[test]
    fn stage5cn_generated_timer_batch_can_reenter_ack_lifecycle() {
        let (settled, request_id, bar_close_ts) = stage5ci_exit_settled();
        let Stage5cSettledPaperStrategy {
            strategy,
            recovery_receipt,
            batch,
            settled_batch_history,
        } = settled;
        let timer = Stage5cTimerResolvedPaperStrategy {
            strategy,
            recovery_receipt,
            resolved_batch_summary: stage5ch_batch_summary(&batch),
            timer_ts_utc_ms: (bar_close_ts + 10) * 1_000,
            generated_intent_batch: Some(batch),
            settled_batch_history,
        };
        let timer_settlement = settle_stage5c_timer_result(timer);
        let resolved = advance_stage5c_paper_loop_once(
            Stage5cPaperLoopState::TimerSettlement(Box::new(timer_settlement)),
            Stage5cPaperLoopEvent::Ack(Box::new(Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            })),
        )
        .expect("generated timer batch reenters ACK lifecycle through coordinator");
        assert_eq!(
            resolved.kind(),
            Stage5cPaperLoopStateKind::IntentLifecycleResolved
        );
    }

    #[test]
    fn stage5cn_ready_timer_settlement_rejects_ack_without_revealing_ready_state() {
        let (settlement, _, _) = stage5cm_ready_subsecond_checkpoint();
        let failure = advance_stage5c_paper_loop_once(
            Stage5cPaperLoopState::TimerSettlement(Box::new(settlement)),
            Stage5cPaperLoopEvent::Ack(Box::new(Stage5cPaperIntentLifecycleInput {
                ack_records: Vec::new(),
            })),
        )
        .expect_err("ready timer settlement is not a generated batch");
        assert_eq!(
            failure.reason(),
            Stage5cPaperLoopError::InvalidTransition {
                state: Stage5cPaperLoopStateKind::TimerSettlement,
                event: Stage5cPaperLoopEventKind::Ack,
            }
        );
        assert_eq!(
            failure.preserved_state().map(Stage5cPaperLoopState::kind),
            Some(Stage5cPaperLoopStateKind::TimerSettlement)
        );
    }

    #[test]
    fn stage5cn_broker_lifecycle_batch_preserves_atomic_stage5cj_semantics() {
        let (settled, tp_request_id, sl_request_id, bar_close_ts) = stage5ci_protective_settled();
        let intent_resolved = advance_stage5c_paper_loop_once(
            Stage5cPaperLoopState::Settled(Box::new(settled)),
            Stage5cPaperLoopEvent::Ack(Box::new(Stage5cPaperIntentLifecycleInput {
                ack_records: vec![
                    Stage5cPaperAckRecord {
                        total_sequence: 1,
                        ack: stage5ci_ack_with(
                            tp_request_id,
                            broker_core::HybridRuntimeAckStatus::Accepted,
                            bar_close_ts + 1,
                        ),
                    },
                    Stage5cPaperAckRecord {
                        total_sequence: 2,
                        ack: stage5ci_ack_with(
                            sl_request_id,
                            broker_core::HybridRuntimeAckStatus::Accepted,
                            bar_close_ts + 1,
                        ),
                    },
                ],
            })),
        )
        .expect("protective TP/SL ACK batch resolves together");
        let broker_resolved = advance_stage5c_paper_loop_once(
            intent_resolved,
            Stage5cPaperLoopEvent::BrokerLifecycleBatch(Box::new(
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![
                        stage5cj_order_event(
                            3,
                            tp_request_id,
                            BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                            "canceled",
                            bar_close_ts + 2,
                        ),
                        stage5cj_stop_event(
                            4,
                            sl_request_id,
                            BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                            "canceled",
                            bar_close_ts + 600,
                            bar_close_ts + 2,
                        ),
                    ],
                },
            )),
        )
        .expect("coordinator passes complete broker-event batch to Stage 5C-j once");
        assert_eq!(
            broker_resolved.kind(),
            Stage5cPaperLoopStateKind::BrokerLifecycleResolved
        );
    }

    #[test]
    fn stage5cn_missing_broker_batch_event_preserves_intent_lifecycle_state() {
        let (settled, tp_request_id, sl_request_id, bar_close_ts) = stage5ci_protective_settled();
        let intent_resolved = advance_stage5c_paper_loop_once(
            Stage5cPaperLoopState::Settled(Box::new(settled)),
            Stage5cPaperLoopEvent::Ack(Box::new(Stage5cPaperIntentLifecycleInput {
                ack_records: vec![
                    Stage5cPaperAckRecord {
                        total_sequence: 1,
                        ack: stage5ci_ack_with(
                            tp_request_id,
                            broker_core::HybridRuntimeAckStatus::Accepted,
                            bar_close_ts + 1,
                        ),
                    },
                    Stage5cPaperAckRecord {
                        total_sequence: 2,
                        ack: stage5ci_ack_with(
                            sl_request_id,
                            broker_core::HybridRuntimeAckStatus::Accepted,
                            bar_close_ts + 1,
                        ),
                    },
                ],
            })),
        )
        .expect("protective ACK batch resolves");
        let failure = advance_stage5c_paper_loop_once(
            intent_resolved,
            Stage5cPaperLoopEvent::BrokerLifecycleBatch(Box::new(
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![stage5cj_order_event(
                        3,
                        tp_request_id,
                        BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                        "working",
                        bar_close_ts + 2,
                    )],
                },
            )),
        )
        .expect_err("missing SL event blocks atomic Stage 5C-j preflight");
        assert_eq!(
            failure.reason(),
            Stage5cPaperLoopError::BrokerLifecycleIncompleteBatch
        );
        assert_eq!(
            failure.preserved_state().map(Stage5cPaperLoopState::kind),
            Some(Stage5cPaperLoopStateKind::IntentLifecycleResolved)
        );
    }

    #[test]
    fn stage5cn_working_only_batch_preserves_state_and_can_retry_full_batch() {
        let (settled, request_id, bar_close_ts) =
            stage5cj_place_entry_settled(crate::BrokerNeutralOrderSide::Buy, 1.0);
        let intent_resolved = advance_stage5c_paper_loop_once(
            Stage5cPaperLoopState::Settled(Box::new(settled)),
            Stage5cPaperLoopEvent::Ack(Box::new(Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            })),
        )
        .expect("entry ACK resolves");
        let blocked = advance_stage5c_paper_loop_once(
            intent_resolved,
            Stage5cPaperLoopEvent::BrokerLifecycleBatch(Box::new(
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![stage5cj_place_order_event(
                        2,
                        request_id,
                        "working",
                        "buy",
                        1.0,
                        bar_close_ts + 2,
                    )],
                },
            )),
        )
        .expect_err("working-only batch is incomplete and must not invoke callbacks");
        assert_eq!(
            blocked.reason(),
            Stage5cPaperLoopError::BrokerLifecycleIncompleteBatch
        );
        let preserved = blocked
            .into_preserved_state()
            .expect("incomplete broker batch preserves intent lifecycle state");
        let completed = advance_stage5c_paper_loop_once(
            preserved,
            Stage5cPaperLoopEvent::BrokerLifecycleBatch(Box::new(
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![
                        stage5cj_place_order_event(
                            2,
                            request_id,
                            "working",
                            "buy",
                            1.0,
                            bar_close_ts + 2,
                        ),
                        stage5cj_place_order_event(
                            3,
                            request_id,
                            "filled",
                            "buy",
                            1.0,
                            bar_close_ts + 3,
                        ),
                        stage5cj_position_event(4, request_id, 1.0, bar_close_ts + 4),
                    ],
                },
            )),
        )
        .expect("caller can retry later with the complete terminal broker batch");
        assert_eq!(
            completed.kind(),
            Stage5cPaperLoopStateKind::BrokerLifecycleResolved
        );
    }

    #[test]
    fn stage5cn_working_filled_position_batch_resolves_as_one_atomic_step() {
        let (settled, request_id, bar_close_ts) =
            stage5cj_place_entry_settled(crate::BrokerNeutralOrderSide::Buy, 1.0);
        let intent_resolved = advance_stage5c_paper_loop_once(
            Stage5cPaperLoopState::Settled(Box::new(settled)),
            Stage5cPaperLoopEvent::Ack(Box::new(Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            })),
        )
        .expect("entry ACK resolves");
        let broker_resolved = advance_stage5c_paper_loop_once(
            intent_resolved,
            Stage5cPaperLoopEvent::BrokerLifecycleBatch(Box::new(
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![
                        stage5cj_place_order_event(
                            2,
                            request_id,
                            "working",
                            "buy",
                            1.0,
                            bar_close_ts + 2,
                        ),
                        stage5cj_place_order_event(
                            3,
                            request_id,
                            "filled",
                            "buy",
                            1.0,
                            bar_close_ts + 3,
                        ),
                        stage5cj_position_event(4, request_id, 1.0, bar_close_ts + 4),
                    ],
                },
            )),
        )
        .expect("complete working -> filled -> position batch resolves");
        match &broker_resolved {
            Stage5cPaperLoopState::BrokerLifecycleResolved(resolved) => {
                assert!(resolved.remaining_lifecycle_expectations().is_empty())
            }
            _ => panic!("expected broker lifecycle resolved state"),
        }
    }

    #[test]
    fn stage5cn_callback_generated_broker_intents_settle_and_reenter_ack_lifecycle() {
        let (settled, request_id, bar_close_ts) = stage5ci_exit_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        let Stage5cResolvedPaperIntentBatchStrategy {
            mut strategy,
            recovery_receipt,
            resolved_batch,
            ack_outcomes,
            settled_batch_history,
        } = resolved;
        let mut state = Strategy::state(&strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                current_owner,
                current_side,
                last_position_qty,
                tp_order_id,
                ..
            } => {
                *active_cycle_id = Some("abc1230001".to_string());
                *current_owner = Some(crate::hybrid_intraday::Owner::MeanReversion);
                *current_side = Some(crate::hybrid_intraday::Side::Long);
                *last_position_qty = 1.0;
                *tp_order_id = Some(BrokerOrderId::new("TP_ORDER_TEST_0001"));
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(&mut strategy, state);
        let resolved = Stage5cResolvedPaperIntentBatchStrategy {
            strategy,
            recovery_receipt,
            resolved_batch,
            ack_outcomes,
            settled_batch_history,
        };
        let broker_resolved = advance_stage5c_paper_loop_once(
            Stage5cPaperLoopState::IntentLifecycleResolved(Box::new(resolved)),
            Stage5cPaperLoopEvent::BrokerLifecycleBatch(Box::new(
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![stage5cj_position_event(
                        2,
                        request_id,
                        0.0,
                        bar_close_ts + 2,
                    )],
                },
            )),
        )
        .expect("flat position cleanup generates broker callback intents");
        let generated_request_ids: Vec<_> = match &broker_resolved {
            Stage5cPaperLoopState::BrokerLifecycleResolved(resolved) => resolved
                .generated_intent_batch()
                .expect("flat position cleanup generates no-send cleanup batch")
                .request_ids()
                .to_vec(),
            _ => panic!("expected broker lifecycle resolved state"),
        };
        assert_eq!(generated_request_ids.len(), 1);

        let generated_settlement = advance_stage5c_paper_loop_once(
            broker_resolved,
            Stage5cPaperLoopEvent::SettleBrokerLifecycleResult,
        )
        .expect("generated broker lifecycle settles to generated intent batch");
        let timer_failure = advance_stage5c_paper_loop_once(
            generated_settlement,
            Stage5cPaperLoopEvent::Timer(Stage5cPaperTimerInput {
                now_ts_utc_ms: (bar_close_ts + 10) * 1_000,
            }),
        )
        .expect_err("generated broker batch cannot advance to timer before ACK");
        assert_eq!(
            timer_failure.reason(),
            Stage5cPaperLoopError::BrokerLifecycleRequiresGeneratedAck
        );
        let generated_settlement = timer_failure
            .into_preserved_state()
            .expect("generated settlement is preserved after timer rejection");

        let ack_resolved = advance_stage5c_paper_loop_once(
            generated_settlement,
            Stage5cPaperLoopEvent::Ack(Box::new(Stage5cPaperIntentLifecycleInput {
                ack_records: generated_request_ids
                    .into_iter()
                    .enumerate()
                    .map(|(idx, request_id)| Stage5cPaperAckRecord {
                        total_sequence: 5 + idx as u64,
                        ack: stage5ci_ack_with(
                            request_id,
                            broker_core::HybridRuntimeAckStatus::Rejected,
                            bar_close_ts + 5,
                        ),
                    })
                    .collect(),
            })),
        )
        .expect("generated broker intents reenter ACK lifecycle");
        assert_eq!(
            ack_resolved.kind(),
            Stage5cPaperLoopStateKind::IntentLifecycleResolved
        );

        let broker_after_generated = advance_stage5c_paper_loop_once(
            ack_resolved,
            Stage5cPaperLoopEvent::BrokerLifecycleBatch(Box::new(
                Stage5cPaperBrokerLifecycleInput {
                    event_records: Vec::new(),
                },
            )),
        )
        .expect("terminal cleanup ACK batch needs no additional broker event");
        let broker_settlement = advance_stage5c_paper_loop_once(
            broker_after_generated,
            Stage5cPaperLoopEvent::SettleBrokerLifecycleResult,
        )
        .expect("clean generated lifecycle settles for timer path");
        assert_eq!(
            broker_settlement.kind(),
            Stage5cPaperLoopStateKind::BrokerLifecycleSettlement
        );
        let timer = advance_stage5c_paper_loop_once(
            broker_settlement,
            Stage5cPaperLoopEvent::Timer(Stage5cPaperTimerInput {
                now_ts_utc_ms: (bar_close_ts + 10) * 1_000,
            }),
        )
        .expect("clean broker settlement advances to bounded timer step");
        let timer_settlement =
            advance_stage5c_paper_loop_once(timer, Stage5cPaperLoopEvent::SettleTimerResult)
                .expect("timer result settles explicitly");
        assert_eq!(
            timer_settlement.kind(),
            Stage5cPaperLoopStateKind::TimerSettlement
        );
    }

    #[test]
    fn stage5cn_timer_blocks_preserve_broker_lifecycle_state() {
        let (settled, tp_request_id, sl_request_id, bar_close_ts) = stage5ci_protective_settled();
        let intent_resolved = advance_stage5c_paper_loop_once(
            Stage5cPaperLoopState::Settled(Box::new(settled)),
            Stage5cPaperLoopEvent::Ack(Box::new(Stage5cPaperIntentLifecycleInput {
                ack_records: vec![
                    Stage5cPaperAckRecord {
                        total_sequence: 1,
                        ack: stage5ci_ack_with(
                            tp_request_id,
                            broker_core::HybridRuntimeAckStatus::Accepted,
                            bar_close_ts + 1,
                        ),
                    },
                    Stage5cPaperAckRecord {
                        total_sequence: 2,
                        ack: stage5ci_ack_with(
                            sl_request_id,
                            broker_core::HybridRuntimeAckStatus::Accepted,
                            bar_close_ts + 1,
                        ),
                    },
                ],
            })),
        )
        .unwrap();
        let unresolved_broker = match intent_resolved {
            Stage5cPaperLoopState::IntentLifecycleResolved(resolved) => {
                resolve_stage5c_paper_broker_lifecycle(
                    *resolved,
                    Stage5cPaperBrokerLifecycleInput {
                        event_records: vec![
                            stage5cj_order_event(
                                3,
                                tp_request_id,
                                BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                                "working",
                                bar_close_ts + 2,
                            ),
                            stage5cj_stop_event(
                                4,
                                sl_request_id,
                                BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                                "working",
                                bar_close_ts + 600,
                                bar_close_ts + 2,
                            ),
                        ],
                    },
                )
                .expect("raw Stage 5C-j facade can still represent unresolved lifecycle")
            }
            _ => panic!("expected intent lifecycle resolved state"),
        };
        let failure = advance_stage5c_paper_loop_once(
            Stage5cPaperLoopState::BrokerLifecycleResolved(Box::new(unresolved_broker)),
            Stage5cPaperLoopEvent::Timer(Stage5cPaperTimerInput {
                now_ts_utc_ms: (bar_close_ts + 10) * 1_000,
            }),
        )
        .expect_err("timer waits for remaining broker lifecycle expectations");
        assert_eq!(
            failure.reason(),
            Stage5cPaperLoopError::Timer(Stage5cPaperTimerError::UnresolvedBrokerLifecycle)
        );
        assert_eq!(
            failure.preserved_state().map(Stage5cPaperLoopState::kind),
            Some(Stage5cPaperLoopStateKind::BrokerLifecycleResolved)
        );

        let (clean_broker, clean_bar_close_ts) = stage5ck_clean_broker_resolved();
        let non_monotonic = advance_stage5c_paper_loop_once(
            Stage5cPaperLoopState::BrokerLifecycleResolved(Box::new(clean_broker)),
            Stage5cPaperLoopEvent::Timer(Stage5cPaperTimerInput {
                now_ts_utc_ms: (clean_bar_close_ts + 1) * 1_000,
            }),
        )
        .expect_err("non-monotonic timer preserves broker lifecycle");
        assert_eq!(
            non_monotonic.reason(),
            Stage5cPaperLoopError::Timer(Stage5cPaperTimerError::NonMonotonicTimer)
        );
        assert_eq!(
            non_monotonic
                .preserved_state()
                .map(Stage5cPaperLoopState::kind),
            Some(Stage5cPaperLoopStateKind::BrokerLifecycleResolved)
        );
    }

    #[test]
    fn stage5cj_position_before_filled_order_does_not_close_lifecycle() {
        let (settled, request_id, bar_close_ts) =
            stage5cj_place_entry_settled(crate::BrokerNeutralOrderSide::Buy, 1.0);
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![
                    stage5cj_position_event(2, request_id, 1.0, bar_close_ts + 2),
                    stage5cj_place_order_event(
                        3,
                        request_id,
                        "filled",
                        "buy",
                        1.0,
                        bar_close_ts + 3,
                    ),
                ],
            },
        )
        .unwrap();
        assert_eq!(
            broker_resolved.remaining_lifecycle_expectations()[0].expected_event_kind,
            Stage5cPaperBrokerEventKind::Position
        );
    }

    #[test]
    fn stage5cj_explicitly_distinguishes_supported_lifecycle_event_kinds() {
        let now = Utc.timestamp_opt(1_783_342_200, 0).single().unwrap();
        let market = stage5cg_market_intent(
            crate::BrokerNeutralOrderSide::Buy,
            crate::BrokerNeutralHybridIntentClass::Entry,
        );
        let place = stage5cg_place_intent();
        let cancel = stage5cg_cancel_intent();
        let replace = crate::BrokerNeutralHybridIntent::Replace {
            order_id: BrokerOrderId::new("ORDER_TEST_0002"),
            new_price: 2231.0,
            new_qty: 1.0,
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::ProtectiveRepair)
        .with_symbol("IMOEXF");
        let create_stop = stage5cg_stop_intent(now.timestamp() + 600);
        let delete_stop = crate::BrokerNeutralHybridIntent::DeleteStopLimit {
            order_id: BrokerStopOrderId::new("STOP_TEST_0002"),
            side: Some(crate::BrokerNeutralOrderSide::Sell),
            check_duplicates: Some(true),
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::CancelCleanup)
        .with_symbol("IMOEXF");
        assert_eq!(
            stage5cj_expected_event_kind(&market),
            Stage5cPaperBrokerEventKind::Position
        );
        assert_eq!(
            stage5cj_expected_event_kind(&place),
            Stage5cPaperBrokerEventKind::Order
        );
        assert_eq!(
            stage5cj_expected_event_kind(&cancel),
            Stage5cPaperBrokerEventKind::Order
        );
        assert_eq!(
            stage5cj_expected_event_kind(&replace),
            Stage5cPaperBrokerEventKind::Order
        );
        assert_eq!(
            stage5cj_expected_event_kind(&create_stop),
            Stage5cPaperBrokerEventKind::StopOrder
        );
        assert_eq!(
            stage5cj_expected_event_kind(&delete_stop),
            Stage5cPaperBrokerEventKind::StopOrder
        );
    }

    // STAGE5G-C-R2CA-R1-AUTHORITY-TESTS-BEGIN: market-terminal-state-coherence-v1
    fn stage5g_r2ca_resolved_market(
        intent_class: crate::BrokerNeutralHybridIntentClass,
        ack_status: broker_core::HybridRuntimeAckStatus,
    ) -> (
        Stage5cResolvedPaperIntentBatchStrategy,
        StrategyRequestId,
        broker_core::HybridRuntimeAttribution,
    ) {
        let (settled, request_id, _) = match intent_class {
            crate::BrokerNeutralHybridIntentClass::Entry => stage5ci_entry_settled(),
            crate::BrokerNeutralHybridIntentClass::Exit => stage5ci_exit_settled(),
            _ => panic!("R2-c-a authority is Entry/Exit MARKET only"),
        };
        let mut resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![Stage5cPaperAckRecord {
                    total_sequence: 1,
                    ack: stage5ci_ack_with(
                        request_id,
                        ack_status,
                        Utc.with_ymd_and_hms(2026, 7, 13, 9, 10, 1)
                            .single()
                            .unwrap()
                            .timestamp(),
                    ),
                }],
            },
        )
        .unwrap();
        let attribution = stage5cj_attribution(match intent_class {
            crate::BrokerNeutralHybridIntentClass::Entry => "ENTRY",
            crate::BrokerNeutralHybridIntentClass::Exit => "EXIT",
            _ => unreachable!(),
        });
        resolved.resolved_batch.records[0].expected_attribution = Some(attribution.clone());
        (resolved, request_id, attribution)
    }

    fn stage5g_r2ca_truth(
        request_id: StrategyRequestId,
        status: broker_core::OrderStatus,
        filled_qty: Decimal,
        position_qty: Decimal,
        side: broker_core::OrderSide,
    ) -> broker_core::BrokerTruthSnapshot {
        let received_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 3)
            .single()
            .unwrap();
        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let broker_order_id = BrokerOrderId::new("ORDER_TEST_ACK_0001");
        let client_order_id = broker_core::ClientOrderId::from_strategy_request(request_id);
        let order = broker_core::BrokerOrderSnapshot {
            account_id: account_id.clone(),
            broker_order_id: Some(broker_order_id.clone()),
            client_order_id: Some(client_order_id.clone()),
            instrument: target(),
            side,
            order_type: broker_core::OrderType::Market,
            time_in_force: None,
            status: status.clone(),
            lifecycle: broker_core::BrokerOrderSnapshot::lifecycle_for(&status),
            qty: Decimal::ONE,
            filled_qty,
            remaining_qty: Some(Decimal::ONE - filled_qty),
            limit_price: None,
            broker_asset_id: None,
            board: None,
            expiration_date: None,
            source_ts: Some(received_ts - chrono::Duration::seconds(1)),
            received_ts,
        };
        let trades = if filled_qty == Decimal::ZERO {
            Vec::new()
        } else {
            vec![broker_core::BrokerTradeSnapshot {
                account_id: account_id.clone(),
                broker_trade_id: broker_core::BrokerTradeId::new("TRADE_TEST_0001"),
                broker_order_id: Some(broker_order_id),
                client_order_id: Some(client_order_id),
                instrument: target(),
                side,
                qty: filled_qty,
                price: Decimal::new(222_750, 2),
                gross_amount: None,
                commission: None,
                broker_asset_id: None,
                board: None,
                expiration_date: None,
                source_ts: received_ts - chrono::Duration::seconds(1),
                received_ts,
            }]
        };
        let positions = if position_qty == Decimal::ZERO {
            Vec::new()
        } else {
            vec![broker_core::BrokerPositionSnapshot {
                account_id: account_id.clone(),
                instrument: target(),
                qty: position_qty,
                avg_price: Some(Decimal::new(222_750, 2)),
                unrealized_pnl: None,
                source_ts: Some(received_ts - chrono::Duration::seconds(1)),
                received_ts,
            }]
        };
        broker_core::BrokerTruthSnapshot {
            account_id,
            orders: vec![order],
            positions,
            cash: None,
            trades,
            instruments: Vec::new(),
            received_ts,
        }
    }

    fn stage5g_r2ca_evidence(
        request_id: StrategyRequestId,
        attribution: broker_core::HybridRuntimeAttribution,
        status: broker_core::OrderStatus,
        filled_qty: Decimal,
        position_qty: Decimal,
        side: broker_core::OrderSide,
    ) -> Stage5cMarketTerminalOrderEvidence {
        Stage5cMarketTerminalOrderEvidence {
            request_id,
            truth: stage5g_r2ca_truth(request_id, status, filled_qty, position_qty, side),
            attribution: Some(attribution),
        }
    }

    fn stage5g_r2ca_validate_and_settle(
        resolved: Stage5cResolvedPaperIntentBatchStrategy,
        evidence: Stage5cMarketTerminalOrderEvidence,
    ) -> Stage5cBrokerLifecycleSettlement {
        let validated = validate_stage5c_market_terminal_outcome(resolved, evidence).unwrap();
        assert!(!validated.evidence_fingerprint().is_empty());
        settle_stage5c_validated_market_terminal_outcome(validated).unwrap()
    }

    fn stage5g_r2ca_ready_for_timer(
        settlement: Stage5cBrokerLifecycleSettlement,
    ) -> Stage5cBrokerLifecycleResolvedPaperStrategy {
        match settlement.inner {
            Stage5cBrokerLifecycleSettlementKind::ReadyForTimer(resolved) => resolved,
            Stage5cBrokerLifecycleSettlementKind::GeneratedIntentBatch(_)
            | Stage5cBrokerLifecycleSettlementKind::UnresolvedBrokerLifecycle(_) => {
                panic!("expected timer-ready terminal outcome")
            }
        }
    }

    fn stage5g_r2ca_generated_intent_batch(
        settlement: Stage5cBrokerLifecycleSettlement,
    ) -> Stage5cSettledPaperStrategy {
        match settlement.inner {
            Stage5cBrokerLifecycleSettlementKind::GeneratedIntentBatch(settled) => settled,
            Stage5cBrokerLifecycleSettlementKind::ReadyForTimer(_)
            | Stage5cBrokerLifecycleSettlementKind::UnresolvedBrokerLifecycle(_) => {
                panic!("expected generated-intent settlement")
            }
        }
    }

    fn stage5g_r2ca_validation_failure(
        resolved: Stage5cResolvedPaperIntentBatchStrategy,
        evidence: Stage5cMarketTerminalOrderEvidence,
    ) -> Stage5cPaperBrokerLifecycleFailure {
        match validate_stage5c_market_terminal_outcome(resolved, evidence) {
            Ok(_) => panic!("terminal evidence unexpectedly validated"),
            Err(failure) => failure,
        }
    }

    #[test]
    fn stage5g_r2ca_zero_fill_entry_resolves_pending_for_accepted_and_confirmed_ack() {
        for (ack_status, status) in [
            (
                broker_core::HybridRuntimeAckStatus::Accepted,
                broker_core::OrderStatus::Rejected,
            ),
            (
                broker_core::HybridRuntimeAckStatus::Confirmed,
                broker_core::OrderStatus::Canceled,
            ),
            (
                broker_core::HybridRuntimeAckStatus::Accepted,
                broker_core::OrderStatus::Expired,
            ),
        ] {
            let (resolved, request_id, attribution) = stage5g_r2ca_resolved_market(
                crate::BrokerNeutralHybridIntentClass::Entry,
                ack_status,
            );
            let output = stage5g_r2ca_ready_for_timer(stage5g_r2ca_validate_and_settle(
                resolved,
                stage5g_r2ca_evidence(
                    request_id,
                    attribution,
                    status,
                    Decimal::ZERO,
                    Decimal::ZERO,
                    broker_core::OrderSide::Buy,
                ),
            ));
            assert!(output.remaining_lifecycle_expectations().is_empty());
            assert_eq!(output.generated_intent_count(), 0);
            let StrategyState::HybridIntradayRuntime {
                last_position_qty,
                pending_entry_owner,
                pending_entry_request_id,
                active_cycle_id,
                safe_mode_close_only,
                ..
            } = Strategy::state(&output.strategy)
            else {
                panic!("expected hybrid state")
            };
            assert_eq!(*last_position_qty, 0.0);
            assert!(pending_entry_owner.is_none());
            assert!(pending_entry_request_id.is_none());
            assert!(active_cycle_id.is_none());
            assert!(*safe_mode_close_only);
            let timer = resolve_stage5c_paper_timer(
                output,
                Stage5cPaperTimerInput {
                    now_ts_utc_ms: Utc
                        .with_ymd_and_hms(2026, 7, 13, 9, 10, 4)
                        .single()
                        .unwrap()
                        .timestamp_millis(),
                },
            )
            .expect("zero-fill entry has no stale lifecycle state");
            assert_eq!(timer.generated_intent_count(), 0);
        }
    }

    #[test]
    fn stage5g_r2ca_zero_fill_exit_keeps_position_and_clears_original_pending() {
        let (resolved, request_id, attribution) = stage5g_r2ca_resolved_market(
            crate::BrokerNeutralHybridIntentClass::Exit,
            broker_core::HybridRuntimeAckStatus::Accepted,
        );
        let output = stage5g_r2ca_ready_for_timer(stage5g_r2ca_validate_and_settle(
            resolved,
            stage5g_r2ca_evidence(
                request_id,
                attribution,
                broker_core::OrderStatus::Expired,
                Decimal::ZERO,
                Decimal::ONE,
                broker_core::OrderSide::Sell,
            ),
        ));
        let StrategyState::HybridIntradayRuntime {
            last_position_qty,
            pending_exit_request_id,
            ..
        } = Strategy::state(&output.strategy)
        else {
            panic!("expected hybrid state")
        };
        assert_eq!(*last_position_qty, 1.0);
        assert!(pending_exit_request_id.is_none());
        assert_eq!(output.generated_intent_count(), 0);
        let timer = resolve_stage5c_paper_timer(
            output,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: Utc
                    .with_ymd_and_hms(2026, 7, 13, 9, 10, 4)
                    .single()
                    .unwrap()
                    .timestamp_millis(),
            },
        )
        .expect("zero-fill exit has no stale lifecycle state");
        assert_eq!(timer.generated_intent_count(), 0);
    }

    #[test]
    fn stage5g_r2ca_partial_entry_and_exit_update_position_and_retain_recovery_intent() {
        for (intent_class, status, position_qty, side) in [
            (
                crate::BrokerNeutralHybridIntentClass::Entry,
                broker_core::OrderStatus::Canceled,
                Decimal::new(4, 1),
                broker_core::OrderSide::Buy,
            ),
            (
                crate::BrokerNeutralHybridIntentClass::Exit,
                broker_core::OrderStatus::Expired,
                Decimal::new(6, 1),
                broker_core::OrderSide::Sell,
            ),
        ] {
            let (resolved, request_id, attribution) = stage5g_r2ca_resolved_market(
                intent_class,
                broker_core::HybridRuntimeAckStatus::Accepted,
            );
            let output = stage5g_r2ca_generated_intent_batch(stage5g_r2ca_validate_and_settle(
                resolved,
                stage5g_r2ca_evidence(
                    request_id,
                    attribution,
                    status,
                    Decimal::new(4, 1),
                    position_qty,
                    side,
                ),
            ));
            let StrategyState::HybridIntradayRuntime {
                last_position_qty,
                pending_entry_request_id,
                pending_exit_request_id,
                safe_mode_close_only,
                ..
            } = Strategy::state(&output.strategy)
            else {
                panic!("expected hybrid state")
            };
            assert!(stage5cj_f64_eq(
                *last_position_qty,
                position_qty.to_f64().unwrap()
            ));
            assert_ne!(*pending_entry_request_id, Some(request_id));
            assert_ne!(*pending_exit_request_id, Some(request_id));
            assert!(pending_exit_request_id.is_some());
            assert!(*safe_mode_close_only);
            assert!(output.intent_batch().intent_count() > 0);
        }
    }

    #[test]
    fn stage5g_r2ca_blocks_rejected_positive_fill_and_preserves_retry_capability() {
        let (resolved, request_id, attribution) = stage5g_r2ca_resolved_market(
            crate::BrokerNeutralHybridIntentClass::Entry,
            broker_core::HybridRuntimeAckStatus::Accepted,
        );
        let failure = stage5g_r2ca_validation_failure(
            resolved,
            stage5g_r2ca_evidence(
                request_id,
                attribution,
                broker_core::OrderStatus::Rejected,
                Decimal::new(4, 1),
                Decimal::new(4, 1),
                broker_core::OrderSide::Buy,
            ),
        );
        assert_eq!(
            failure.reason(),
            Stage5cPaperBrokerLifecycleError::IntentFieldMismatch
        );
        assert!(failure.into_blocked().is_some());
    }

    #[test]
    fn stage5g_r2ca_blocks_wrong_side_quantity_and_attribution() {
        let (resolved, request_id, attribution) = stage5g_r2ca_resolved_market(
            crate::BrokerNeutralHybridIntentClass::Entry,
            broker_core::HybridRuntimeAckStatus::Accepted,
        );
        let mut evidence = stage5g_r2ca_evidence(
            request_id,
            attribution,
            broker_core::OrderStatus::Canceled,
            Decimal::ZERO,
            Decimal::ZERO,
            broker_core::OrderSide::Buy,
        );
        evidence.truth.orders[0].side = broker_core::OrderSide::Sell;
        assert_eq!(
            stage5g_r2ca_validation_failure(resolved, evidence).reason(),
            Stage5cPaperBrokerLifecycleError::IntentFieldMismatch
        );

        let (resolved, request_id, attribution) = stage5g_r2ca_resolved_market(
            crate::BrokerNeutralHybridIntentClass::Entry,
            broker_core::HybridRuntimeAckStatus::Accepted,
        );
        let mut evidence = stage5g_r2ca_evidence(
            request_id,
            attribution,
            broker_core::OrderStatus::Canceled,
            Decimal::ZERO,
            Decimal::ZERO,
            broker_core::OrderSide::Buy,
        );
        evidence.truth.orders[0].qty = Decimal::new(2, 0);
        assert_eq!(
            stage5g_r2ca_validation_failure(resolved, evidence).reason(),
            Stage5cPaperBrokerLifecycleError::IntentFieldMismatch
        );

        let (resolved, request_id, attribution) = stage5g_r2ca_resolved_market(
            crate::BrokerNeutralHybridIntentClass::Entry,
            broker_core::HybridRuntimeAckStatus::Accepted,
        );
        let mut evidence = stage5g_r2ca_evidence(
            request_id,
            attribution,
            broker_core::OrderStatus::Canceled,
            Decimal::ZERO,
            Decimal::ZERO,
            broker_core::OrderSide::Buy,
        );
        evidence.attribution = None;
        assert_eq!(
            stage5g_r2ca_validation_failure(resolved, evidence).reason(),
            Stage5cPaperBrokerLifecycleError::AttributionRoleMismatch
        );
    }

    #[test]
    fn stage5g_r2ca_blocks_partial_without_position_and_duplicate_terminal_order() {
        let (resolved, request_id, attribution) = stage5g_r2ca_resolved_market(
            crate::BrokerNeutralHybridIntentClass::Entry,
            broker_core::HybridRuntimeAckStatus::Accepted,
        );
        let evidence = stage5g_r2ca_evidence(
            request_id,
            attribution,
            broker_core::OrderStatus::Canceled,
            Decimal::new(4, 1),
            Decimal::ZERO,
            broker_core::OrderSide::Buy,
        );
        assert_eq!(
            stage5g_r2ca_validation_failure(resolved, evidence).reason(),
            Stage5cPaperBrokerLifecycleError::PositionSideMismatch
        );

        let (resolved, request_id, attribution) = stage5g_r2ca_resolved_market(
            crate::BrokerNeutralHybridIntentClass::Entry,
            broker_core::HybridRuntimeAckStatus::Accepted,
        );
        let mut evidence = stage5g_r2ca_evidence(
            request_id,
            attribution,
            broker_core::OrderStatus::Expired,
            Decimal::ZERO,
            Decimal::ZERO,
            broker_core::OrderSide::Buy,
        );
        evidence.truth.orders.push(evidence.truth.orders[0].clone());
        assert_eq!(
            stage5g_r2ca_validation_failure(resolved, evidence).reason(),
            Stage5cPaperBrokerLifecycleError::BrokerOrderIdMismatch
        );
    }

    #[test]
    fn stage5g_r2ca_validation_failure_preserves_exact_retry_capability() {
        let (resolved, request_id, attribution) = stage5g_r2ca_resolved_market(
            crate::BrokerNeutralHybridIntentClass::Entry,
            broker_core::HybridRuntimeAckStatus::Confirmed,
        );
        let mut stale = stage5g_r2ca_evidence(
            request_id,
            attribution.clone(),
            broker_core::OrderStatus::Canceled,
            Decimal::ZERO,
            Decimal::ZERO,
            broker_core::OrderSide::Buy,
        );
        stale.truth.received_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap();
        let blocked = stage5g_r2ca_validation_failure(resolved, stale)
            .into_blocked()
            .expect("validation failure must preserve resolved capability");
        assert_eq!(
            blocked.reason(),
            Stage5cPaperBrokerLifecycleError::EventTimestampBeforeAck
        );

        let corrected = stage5g_r2ca_evidence(
            request_id,
            attribution,
            broker_core::OrderStatus::Canceled,
            Decimal::ZERO,
            Decimal::ZERO,
            broker_core::OrderSide::Buy,
        );
        let output = stage5g_r2ca_ready_for_timer(stage5g_r2ca_validate_and_settle(
            blocked.into_resolved(),
            corrected,
        ));
        let StrategyState::HybridIntradayRuntime {
            pending_entry_request_id,
            last_position_qty,
            ..
        } = Strategy::state(&output.strategy)
        else {
            panic!("expected hybrid state")
        };
        assert!(pending_entry_request_id.is_none());
        assert_eq!(*last_position_qty, 0.0);
    }

    #[test]
    fn stage5g_r2ca_rejects_non_monotonic_order_trade_and_position_chronology() {
        let (resolved, request_id, attribution) = stage5g_r2ca_resolved_market(
            crate::BrokerNeutralHybridIntentClass::Entry,
            broker_core::HybridRuntimeAckStatus::Accepted,
        );
        let mut bad_order = stage5g_r2ca_evidence(
            request_id,
            attribution,
            broker_core::OrderStatus::Canceled,
            Decimal::ZERO,
            Decimal::ZERO,
            broker_core::OrderSide::Buy,
        );
        bad_order.truth.orders[0].source_ts =
            Some(bad_order.truth.orders[0].received_ts + chrono::Duration::seconds(1));
        assert_eq!(
            stage5g_r2ca_validation_failure(resolved, bad_order).reason(),
            Stage5cPaperBrokerLifecycleError::IntentFieldMismatch
        );

        let (resolved, request_id, attribution) = stage5g_r2ca_resolved_market(
            crate::BrokerNeutralHybridIntentClass::Entry,
            broker_core::HybridRuntimeAckStatus::Accepted,
        );
        let mut bad_trade = stage5g_r2ca_evidence(
            request_id,
            attribution,
            broker_core::OrderStatus::Canceled,
            Decimal::new(4, 1),
            Decimal::new(4, 1),
            broker_core::OrderSide::Buy,
        );
        bad_trade.truth.trades[0].source_ts =
            bad_trade.truth.trades[0].received_ts + chrono::Duration::seconds(1);
        assert_eq!(
            stage5g_r2ca_validation_failure(resolved, bad_trade).reason(),
            Stage5cPaperBrokerLifecycleError::IntentFieldMismatch
        );

        let (resolved, request_id, attribution) = stage5g_r2ca_resolved_market(
            crate::BrokerNeutralHybridIntentClass::Entry,
            broker_core::HybridRuntimeAckStatus::Accepted,
        );
        let mut bad_position = stage5g_r2ca_evidence(
            request_id,
            attribution,
            broker_core::OrderStatus::Expired,
            Decimal::new(4, 1),
            Decimal::new(4, 1),
            broker_core::OrderSide::Buy,
        );
        bad_position.truth.positions[0].source_ts =
            Some(bad_position.truth.positions[0].received_ts + chrono::Duration::seconds(1));
        assert_eq!(
            stage5g_r2ca_validation_failure(resolved, bad_position).reason(),
            Stage5cPaperBrokerLifecycleError::InstrumentMismatch
        );
    }
    // STAGE5G-C-R2CA-R1-AUTHORITY-TESTS-END: market-terminal-state-coherence-v1
}

// STAGE5G-C-R2CA-R2-AUTHORITY-TESTS-BEGIN: deterministic-terminal-fill-boundary-v1
#[cfg(test)]
mod stage5g_r2ca_r2_tests {
    use super::*;
    use broker_core::command::{CommandAckReason, CommandAckReasonCode, CommandAckStatus};
    use broker_core::{
        BrokerTradeId, ClientOrderId, CommandAck, Exchange, Market, OrderSide, OrderStatus,
    };
    use chrono::{Duration, NaiveDate, NaiveTime, TimeZone, Timelike};
    use rust_decimal::Decimal;

    use crate::hybrid_intraday::{
        BreakoutEodMode, HybridOrchestratorConfig, IntradayBreakoutConfig, MeanReversionConfig,
        MinRangeMode, Owner, Side,
    };
    use crate::hybrid_intraday_runtime::{
        HybridIntradayProfile, HybridIntradayRuntimeConfig, MeanReversionVariant, MrGatePolicy,
        RiskGateMode,
    };
    use crate::runtime_compat::{
        BarEvent, DataOrigin, MarketBuyAndCloseLiveOrderStyle, PaperExecutionMode,
        RiskGateRuntimeState,
    };
    use crate::{
        apply_stage5g_mock_ack, attach_stage5g_mock_ack_session, BrokerNeutralHybridIntentClass,
        BrokerNeutralOrderSide, Stage5gMockAckEvent, Stage5gMockAckSessionInput,
        Stage5gMockIntentAction, Stage5gMockIntentBinding, Stage5gMockPlaceKind,
    };

    const BAR_CLOSE_TS: i64 = 1_767_679_800;
    const BROKER_ORDER_ID: &str = "FINAM_STAGE5G_R2CA_R2_ORDER_0001";

    #[derive(Clone, Copy)]
    enum AckPath {
        Accepted,
        SubmittedRecovered,
    }

    struct SourceFixture {
        resolved: Stage5cResolvedPaperIntentBatchStrategy,
        request_id: StrategyRequestId,
        attribution: broker_core::HybridRuntimeAttribution,
        intent_class: BrokerNeutralHybridIntentClass,
        side: BrokerNeutralOrderSide,
        order_qty: Decimal,
        pre_position_qty: Decimal,
        bar_close_ts: i64,
    }

    fn target() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn context(close_time_utc: i64, position_qty: f64) -> StrategyCtx {
        StrategyCtx {
            strategy_id: "hybrid_imoexf".to_string(),
            portfolio: "ACC_TEST_0001".to_string(),
            exchange: "MOEX".to_string(),
            symbol: "IMOEXF".to_string(),
            tick_size: 0.5,
            trade_mode: TradeMode::Paper,
            paper_execution_mode: PaperExecutionMode::LiveOnly,
            allow_live_orders: false,
            gateway_phase: GatewayPhase::LiveReady,
            position_qty: Some(position_qty),
            event_ts_utc: close_time_utc,
            now_ts_utc: close_time_utc,
            last_bar_ts: Some(close_time_utc),
        }
    }

    fn accepted_stage5f_entry_strategy() -> HybridIntradayRuntimeStrategy {
        let config = HybridIntradayRuntimeConfig {
            symbol: "IMOEXF".to_string(),
            profile: HybridIntradayProfile::ImoexfPrimaryRiskgateHigh180Lb120,
            mr_variant: MeanReversionVariant::High180,
            mr_gate_policy: MrGatePolicy::ShadowPnlLb120Positive,
            risk_gate_mode: RiskGateMode::NormalAppend,
            risk_gate_seed_file: None,
            risk_gate_ledger_key: None,
            model_session_start_time: NaiveTime::from_hms_opt(9, 0, 0),
            model_session_end_time: NaiveTime::from_hms_opt(23, 49, 59),
            qty: 3.0,
            live_order_style: MarketBuyAndCloseLiveOrderStyle::Market,
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
            partial_entry_fill_timeout_ms: 3_000,
            mr_config: MeanReversionConfig {
                exit_offset: Duration::minutes(10),
                ..MeanReversionConfig::default()
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
                breakout_overnight_exit_time: NaiveTime::from_hms_opt(9, 30, 0)
                    .expect("accepted Stage 5F overnight exit time"),
            },
        };
        let strategy = HybridIntradayRuntimeStrategy::new(config);
        assert_eq!(
            strategy.stage5d_canonical_config_fingerprint(),
            "stage5d_cfg_sha256:56141846cb180b8a224a1db7e1f5188c99c28f0fab88a27ebe65fbcb9d7cf626"
        );
        strategy
    }

    fn accepted_stage5f_entry_settled() -> Stage5cSettledPaperStrategy {
        let mut strategy = accepted_stage5f_entry_strategy();
        Strategy::on_risk_gate_state(
            &mut strategy,
            &RiskGateRuntimeState {
                profile_id: "imoexf_primary_high180_lb120".to_string(),
                last_finalized_session_date: NaiveDate::from_ymd_opt(2026, 1, 5),
                rolling_sum_lb120: Some(158.6),
                mr_enabled_current_session: Some(true),
                mr_enabled_next_session: Some(true),
                ledger_rows_count: 221,
            },
        );
        for (close_time_utc, high, low) in [
            (BAR_CLOSE_TS - 86_400 - 600, 102.0, 98.0),
            (BAR_CLOSE_TS - 86_400, 101.0, 99.0),
        ] {
            assert!(Strategy::on_bar(
                &mut strategy,
                &context(close_time_utc, 0.0),
                &BarEvent {
                    symbol: "IMOEXF".to_string(),
                    close_time_utc,
                    o: 100.0,
                    h: high,
                    l: low,
                    close: 100.0,
                    v: 1.0,
                    origin: DataOrigin::Replay,
                },
            )
            .is_empty());
        }
        let bar = broker_core::HybridRuntimeBarEvent {
            instrument: target(),
            close_time_utc: BAR_CLOSE_TS,
            open: 99.7,
            high: 102.0,
            low: 99.7,
            close: 99.7,
            volume: 10.0,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            is_final: true,
            timeframe_sec: 600,
        };
        let lifecycle_now = Utc.timestamp_opt(BAR_CLOSE_TS - 30, 0).single().unwrap();
        let (recovered, accepted) = stage5f_test_seams::sequence_inputs_from_owned_strategy(
            strategy,
            "hybrid_imoexf".to_string(),
            BrokerAccountId::new("ACC_TEST_0001"),
            target(),
            0.5,
            Decimal::ZERO,
            lifecycle_now,
            BAR_CLOSE_TS - 600,
            bar,
        );
        let semantic = apply_stage5c_semantic_bar_at(
            recovered,
            accepted,
            Utc.timestamp_opt(BAR_CLOSE_TS + 1, 0).single().unwrap(),
        )
        .expect("accepted Stage 5F Entry semantic callback");
        settle_stage5c_semantic_result(semantic).expect("accepted Stage 5F Entry intent escrow")
    }

    fn production_exit_strategy() -> HybridIntradayRuntimeStrategy {
        let utc_bar_close = Utc.timestamp_opt(BAR_CLOSE_TS, 0).single().unwrap();
        let timezone_offset_hours = 9 - i32::try_from(utc_bar_close.hour()).unwrap();
        let local_bar_close = utc_bar_close + Duration::hours(i64::from(timezone_offset_hours));
        HybridIntradayRuntimeStrategy::new(HybridIntradayRuntimeConfig {
            symbol: "IMOEXF".to_string(),
            profile: HybridIntradayProfile::BaselineRuntimeHybrid,
            mr_variant: MeanReversionVariant::Author41BoundaryShort,
            mr_gate_policy: MrGatePolicy::Disabled,
            risk_gate_mode: RiskGateMode::Disabled,
            risk_gate_seed_file: None,
            risk_gate_ledger_key: None,
            model_session_start_time: Some((local_bar_close - Duration::minutes(10)).time()),
            model_session_end_time: Some((local_bar_close + Duration::hours(1)).time()),
            qty: 1.0,
            live_order_style: MarketBuyAndCloseLiveOrderStyle::Market,
            tick_size: 0.5,
            marketable_limit_offset_ticks: 0,
            timezone_offset_hours,
            session_close_hour: 23,
            session_close_minute: 49,
            weekends_off: false,
            stop_end_buffer_sec: 60,
            repair_deadline_sec: 180,
            sl_escalate_timeout_sec: 30,
            max_repair_retries: 3,
            repair_backoff_base_sec: 5,
            repair_backoff_max_sec: 60,
            pending_timeout_sec: 30,
            partial_entry_fill_timeout_ms: 3_000,
            mr_config: MeanReversionConfig::default(),
            breakout_config: IntradayBreakoutConfig {
                exclude_weekends: false,
                wait_hours: 0.0,
                ..IntradayBreakoutConfig::default()
            },
            orchestrator_config: HybridOrchestratorConfig::default(),
        })
    }

    fn production_exit_settled(bracket_started_ms: Option<i64>) -> Stage5cSettledPaperStrategy {
        let mut strategy = production_exit_strategy();
        for (close_time_utc, high, low) in [
            (BAR_CLOSE_TS - 86_400 - 600, 2630.0, 2570.0),
            (BAR_CLOSE_TS - 86_400, 2620.0, 2580.0),
        ] {
            assert!(Strategy::on_bar(
                &mut strategy,
                &context(close_time_utc, 0.0),
                &BarEvent {
                    symbol: "IMOEXF".to_string(),
                    close_time_utc,
                    o: 2600.0,
                    h: high,
                    l: low,
                    close: 2600.0,
                    v: 1.0,
                    origin: DataOrigin::Replay,
                },
            )
            .is_empty());
        }
        let mut state = Strategy::state(&strategy).clone();
        let StrategyState::HybridIntradayRuntime {
            active_cycle_id,
            last_position_qty,
            current_owner,
            current_side,
            ..
        } = &mut state
        else {
            panic!("production Exit fixture requires hybrid runtime state")
        };
        *active_cycle_id = Some("abc1230001".to_string());
        *last_position_qty = 1.0;
        *current_owner = Some(Owner::IntradayBreakout);
        *current_side = Some(Side::Long);
        Strategy::set_state(&mut strategy, state);
        if let Some(started_ms) = bracket_started_ms {
            let mut extension = strategy
                .stage5d_export_runtime_private_extension()
                .expect("export bracket timer fixture");
            extension.bracket_reconciliation_timer = Some(
                crate::stage5d_persistence::Stage5dBracketReconciliationTimer {
                    bracket_terminal_reconcile_started_ms: started_ms,
                },
            );
            strategy
                .stage5d_apply_runtime_private_extension(&extension)
                .expect("apply bracket timer fixture");
        }
        let bar = broker_core::HybridRuntimeBarEvent {
            instrument: target(),
            close_time_utc: BAR_CLOSE_TS,
            open: 2601.0,
            high: 2602.0,
            low: 2599.0,
            close: 2601.0,
            volume: 1.0,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            is_final: true,
            timeframe_sec: 600,
        };
        let lifecycle_now = Utc.timestamp_opt(BAR_CLOSE_TS - 30, 0).single().unwrap();
        let (recovered, accepted) = stage5f_test_seams::sequence_inputs_from_owned_strategy(
            strategy,
            "hybrid_imoexf".to_string(),
            BrokerAccountId::new("ACC_TEST_0001"),
            target(),
            0.5,
            Decimal::ONE,
            lifecycle_now,
            BAR_CLOSE_TS - 600,
            bar,
        );
        let semantic = apply_stage5c_semantic_bar_at(
            recovered,
            accepted,
            Utc.timestamp_opt(BAR_CLOSE_TS + 1, 0).single().unwrap(),
        )
        .expect("source-reachable Stage 5F Exit semantic callback");
        settle_stage5c_semantic_result(semantic)
            .expect("source-reachable Stage 5F Exit intent escrow")
    }

    fn mock_ack_event(
        request_id: StrategyRequestId,
        side: BrokerNeutralOrderSide,
        status: CommandAckStatus,
        broker_order_id: Option<&str>,
        sequence: u64,
    ) -> Stage5gMockAckEvent {
        Stage5gMockAckEvent {
            total_sequence: sequence,
            intent_request_id: request_id,
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            instrument: target(),
            action: Stage5gMockIntentAction::Place {
                place_kind: Stage5gMockPlaceKind::Market,
            },
            side: Some(side),
            ack: CommandAck {
                request_id,
                client_order_id: Some(ClientOrderId::from_strategy_request(request_id)),
                broker_order_id: broker_order_id.map(BrokerOrderId::new),
                status,
                reason: (status == CommandAckStatus::Recovered)
                    .then(|| CommandAckReason::new(CommandAckReasonCode::RecoveredByBrokerTruth)),
                received_ts: Utc
                    .timestamp_opt(BAR_CLOSE_TS + i64::try_from(sequence).unwrap(), 0)
                    .single()
                    .unwrap(),
            },
        }
    }

    fn resolve_source_fixture(
        settled: Stage5cSettledPaperStrategy,
        ack_path: AckPath,
    ) -> SourceFixture {
        let mut projections = settled.stage5g_source_intent_projections();
        assert_eq!(projections.len(), 1, "source witness must emit one intent");
        let projection = projections.remove(0);
        assert_eq!(projection.base_action, Stage5gSourceBaseAction::Market);
        let side = projection.side.expect("MARKET source side");
        let order_qty = Decimal::from_f64_retain(
            projection
                .target_qty
                .expect("MARKET source target quantity"),
        )
        .expect("exact fixture quantity");
        let pre_position_qty = Decimal::from_f64_retain(projection.pre_position_qty)
            .expect("exact fixture pre-position");
        let attribution = projection
            .expected_attribution
            .clone()
            .expect("source-owned attribution");
        let request_id = projection.request_id;
        let intent_class = projection.intent_class;
        let action = Stage5gMockIntentAction::Place {
            place_kind: Stage5gMockPlaceKind::Market,
        };
        let session = attach_stage5g_mock_ack_session(
            settled,
            Stage5gMockAckSessionInput {
                intent_bindings: vec![Stage5gMockIntentBinding {
                    request_id,
                    intent_class,
                    action,
                    side: Some(side),
                }],
                lifecycle_expires_at_ts_utc: BAR_CLOSE_TS + 300,
            },
        )
        .expect("Stage 5G-b source attachment");
        let stage5g_resolved = match ack_path {
            AckPath::Accepted => apply_stage5g_mock_ack(
                session,
                mock_ack_event(
                    request_id,
                    side,
                    CommandAckStatus::Accepted,
                    Some(BROKER_ORDER_ID),
                    1,
                ),
            )
            .expect("Accepted ACK source path")
            .into_resolved()
            .expect("Accepted resolves one-intent session"),
            AckPath::SubmittedRecovered => {
                let awaiting = apply_stage5g_mock_ack(
                    session,
                    mock_ack_event(request_id, side, CommandAckStatus::Submitted, None, 1),
                )
                .expect("Submitted ACK source path")
                .into_awaiting()
                .expect("Submitted awaits broker identity");
                apply_stage5g_mock_ack(
                    awaiting,
                    mock_ack_event(
                        request_id,
                        side,
                        CommandAckStatus::Recovered,
                        Some(BROKER_ORDER_ID),
                        2,
                    ),
                )
                .expect("Recovered ACK source path")
                .into_resolved()
                .expect("Recovered resolves one-intent session")
            }
        };
        let (resolved, _stage5g_context) = stage5g_resolved.into_stage5g_c_parts();
        SourceFixture {
            resolved,
            request_id,
            attribution,
            intent_class,
            side,
            order_qty,
            pre_position_qty,
            bar_close_ts: BAR_CLOSE_TS,
        }
    }

    fn entry_fixture(ack_path: AckPath) -> SourceFixture {
        let fixture = resolve_source_fixture(accepted_stage5f_entry_settled(), ack_path);
        assert_eq!(fixture.intent_class, BrokerNeutralHybridIntentClass::Entry);
        assert_eq!(fixture.pre_position_qty, Decimal::ZERO);
        fixture
    }

    fn exit_fixture(ack_path: AckPath, bracket_started_ms: Option<i64>) -> SourceFixture {
        let fixture = resolve_source_fixture(production_exit_settled(bracket_started_ms), ack_path);
        assert_eq!(fixture.intent_class, BrokerNeutralHybridIntentClass::Exit);
        assert_eq!(fixture.pre_position_qty, Decimal::ONE);
        fixture
    }

    fn terminal_evidence(
        fixture: &SourceFixture,
        status: OrderStatus,
        filled_qty: Decimal,
        target_position_qty: Decimal,
        event_offset_seconds: i64,
    ) -> Stage5cMarketTerminalOrderEvidence {
        let event_ts = Utc
            .timestamp_opt(fixture.bar_close_ts + event_offset_seconds, 0)
            .single()
            .unwrap();
        let received_ts = event_ts + Duration::seconds(1);
        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let broker_order_id = BrokerOrderId::new(BROKER_ORDER_ID);
        let client_order_id = ClientOrderId::from_strategy_request(fixture.request_id);
        let order_side = match fixture.side {
            BrokerNeutralOrderSide::Buy => OrderSide::Buy,
            BrokerNeutralOrderSide::Sell => OrderSide::Sell,
        };
        let order = broker_core::BrokerOrderSnapshot {
            account_id: account_id.clone(),
            broker_order_id: Some(broker_order_id.clone()),
            client_order_id: Some(client_order_id.clone()),
            instrument: target(),
            side: order_side,
            order_type: broker_core::OrderType::Market,
            time_in_force: None,
            lifecycle: broker_core::BrokerOrderSnapshot::lifecycle_for(&status),
            status,
            qty: fixture.order_qty,
            filled_qty,
            remaining_qty: Some(fixture.order_qty - filled_qty),
            limit_price: None,
            broker_asset_id: None,
            board: None,
            expiration_date: None,
            source_ts: Some(event_ts),
            received_ts: event_ts,
        };
        let trades = if filled_qty == Decimal::ZERO {
            Vec::new()
        } else {
            vec![broker_core::BrokerTradeSnapshot {
                account_id: account_id.clone(),
                broker_trade_id: BrokerTradeId::new("FINAM_STAGE5G_R2CA_R2_TRADE_0001"),
                broker_order_id: Some(broker_order_id),
                client_order_id: Some(client_order_id),
                instrument: target(),
                side: order_side,
                qty: filled_qty,
                price: Decimal::new(222_750, 2),
                gross_amount: None,
                commission: None,
                broker_asset_id: None,
                board: None,
                expiration_date: None,
                source_ts: event_ts,
                received_ts: event_ts,
            }]
        };
        let positions = if target_position_qty == Decimal::ZERO {
            Vec::new()
        } else {
            vec![broker_core::BrokerPositionSnapshot {
                account_id: account_id.clone(),
                instrument: target(),
                qty: target_position_qty,
                avg_price: Some(Decimal::new(222_750, 2)),
                unrealized_pnl: None,
                source_ts: Some(event_ts),
                received_ts: event_ts,
            }]
        };
        Stage5cMarketTerminalOrderEvidence {
            request_id: fixture.request_id,
            truth: broker_core::BrokerTruthSnapshot {
                account_id,
                orders: vec![order],
                positions,
                cash: None,
                trades,
                instruments: Vec::new(),
                received_ts,
            },
            attribution: Some(fixture.attribution.clone()),
        }
    }

    fn validate_and_settle(
        fixture: SourceFixture,
        evidence: Stage5cMarketTerminalOrderEvidence,
    ) -> Stage5cBrokerLifecycleSettlement {
        let validated = validate_stage5c_market_terminal_outcome_r2(fixture.resolved, evidence)
            .expect("R2 terminal evidence validation");
        settle_stage5c_validated_market_terminal_outcome_r2(validated)
            .expect("R2 transactional terminal settlement")
    }

    fn validation_blocked(
        resolved: Stage5cResolvedPaperIntentBatchStrategy,
        evidence: Stage5cMarketTerminalOrderEvidence,
    ) -> Box<Stage5cMarketTerminalR2Blocked> {
        match validate_stage5c_market_terminal_outcome_r2(resolved, evidence) {
            Ok(_) => panic!("terminal evidence unexpectedly validated"),
            Err(blocked) => blocked,
        }
    }

    fn ready_for_timer(
        settlement: Stage5cBrokerLifecycleSettlement,
    ) -> Stage5cBrokerLifecycleResolvedPaperStrategy {
        match settlement.inner {
            Stage5cBrokerLifecycleSettlementKind::ReadyForTimer(resolved) => resolved,
            Stage5cBrokerLifecycleSettlementKind::GeneratedIntentBatch(_)
            | Stage5cBrokerLifecycleSettlementKind::UnresolvedBrokerLifecycle(_) => {
                panic!("expected ReadyForTimer")
            }
        }
    }

    fn generated_batch(
        settlement: Stage5cBrokerLifecycleSettlement,
    ) -> Stage5cSettledPaperStrategy {
        match settlement.inner {
            Stage5cBrokerLifecycleSettlementKind::GeneratedIntentBatch(settled) => settled,
            Stage5cBrokerLifecycleSettlementKind::ReadyForTimer(_)
            | Stage5cBrokerLifecycleSettlementKind::UnresolvedBrokerLifecycle(_) => {
                panic!("expected GeneratedIntentBatch")
            }
        }
    }

    #[test]
    fn r2_source_path_zero_fill_entry_rejected_and_recovered_canceled_are_timer_ready() {
        for (ack_path, status) in [
            (AckPath::Accepted, OrderStatus::Rejected),
            (AckPath::SubmittedRecovered, OrderStatus::Canceled),
        ] {
            let fixture = entry_fixture(ack_path);
            let evidence = terminal_evidence(&fixture, status, Decimal::ZERO, Decimal::ZERO, 3);
            let output = ready_for_timer(validate_and_settle(fixture, evidence));
            let StrategyState::HybridIntradayRuntime {
                last_position_qty,
                pending_entry_owner,
                pending_entry_request_id,
                active_cycle_id,
                safe_mode_close_only,
                ..
            } = Strategy::state(output.strategy())
            else {
                panic!("expected hybrid runtime state")
            };
            assert_eq!(*last_position_qty, 0.0);
            assert!(pending_entry_owner.is_none());
            assert!(pending_entry_request_id.is_none());
            assert!(active_cycle_id.is_none());
            assert!(*safe_mode_close_only);
            assert_eq!(output.generated_intent_count(), 0);
        }
    }

    #[test]
    fn r2_source_path_zero_fill_exit_expired_preserves_owned_position() {
        let fixture = exit_fixture(AckPath::Accepted, None);
        let evidence = terminal_evidence(
            &fixture,
            OrderStatus::Expired,
            Decimal::ZERO,
            Decimal::ONE,
            3,
        );
        let output = ready_for_timer(validate_and_settle(fixture, evidence));
        let StrategyState::HybridIntradayRuntime {
            last_position_qty,
            current_owner,
            current_side,
            pending_exit_request_id,
            active_cycle_id,
            ..
        } = Strategy::state(output.strategy())
        else {
            panic!("expected hybrid runtime state")
        };
        assert_eq!(*last_position_qty, 1.0);
        assert_eq!(*current_owner, Some(Owner::IntradayBreakout));
        assert_eq!(*current_side, Some(Side::Long));
        assert!(pending_exit_request_id.is_none());
        assert_eq!(active_cycle_id.as_deref(), Some("abc1230001"));
        assert_eq!(output.generated_intent_count(), 0);
    }

    #[test]
    fn r2_source_path_partial_entry_canceled_restores_owner_cycle_and_escrows_exit() {
        let fixture = entry_fixture(AckPath::Accepted);
        let evidence = terminal_evidence(
            &fixture,
            OrderStatus::Canceled,
            Decimal::ONE,
            Decimal::ONE,
            3,
        );
        let output = generated_batch(validate_and_settle(fixture, evidence));
        let StrategyState::HybridIntradayRuntime {
            last_position_qty,
            current_owner,
            current_side,
            pending_entry_request_id,
            pending_exit_request_id,
            active_cycle_id,
            safe_mode_close_only,
            ..
        } = Strategy::state(&output.strategy)
        else {
            panic!("expected hybrid runtime state")
        };
        assert_eq!(*last_position_qty, 1.0);
        assert_eq!(*current_owner, Some(Owner::MeanReversion));
        assert_eq!(*current_side, Some(Side::Long));
        assert!(pending_entry_request_id.is_none());
        assert!(pending_exit_request_id.is_some());
        assert!(active_cycle_id.is_some());
        assert!(*safe_mode_close_only);
        assert!(output.intent_batch().intent_count() >= 1);
        assert!(output
            .stage5g_source_intent_projections()
            .iter()
            .any(|projection| projection.intent_class == BrokerNeutralHybridIntentClass::Exit));
    }

    #[test]
    fn r2_source_path_partial_exit_outside_grace_escrows_recovery_exit() {
        let fixture = exit_fixture(AckPath::Accepted, Some((BAR_CLOSE_TS - 10) * 1_000));
        let evidence = terminal_evidence(
            &fixture,
            OrderStatus::Expired,
            Decimal::new(4, 1),
            Decimal::new(6, 1),
            3,
        );
        let output = generated_batch(validate_and_settle(fixture, evidence));
        let StrategyState::HybridIntradayRuntime {
            last_position_qty,
            current_owner,
            pending_exit_request_id,
            active_cycle_id,
            safe_mode_close_only,
            ..
        } = Strategy::state(&output.strategy)
        else {
            panic!("expected hybrid runtime state")
        };
        assert_eq!(*last_position_qty, 0.6);
        assert_eq!(*current_owner, Some(Owner::IntradayBreakout));
        assert!(pending_exit_request_id.is_some());
        assert_eq!(active_cycle_id.as_deref(), Some("abc1230001"));
        assert!(*safe_mode_close_only);
        assert!(output.intent_batch().intent_count() >= 1);
    }

    #[test]
    fn r2_partial_exit_inside_grace_is_timer_ready_then_timer_escrows_residual() {
        let started_ms = (BAR_CLOSE_TS + 2) * 1_000;
        let fixture = exit_fixture(AckPath::Accepted, Some(started_ms));
        let evidence = terminal_evidence(
            &fixture,
            OrderStatus::Expired,
            Decimal::new(4, 1),
            Decimal::new(6, 1),
            3,
        );
        let output = ready_for_timer(validate_and_settle(fixture, evidence));
        let StrategyState::HybridIntradayRuntime {
            last_position_qty,
            pending_exit_request_id,
            ..
        } = Strategy::state(output.strategy())
        else {
            panic!("expected hybrid runtime state")
        };
        assert_eq!(*last_position_qty, 0.6);
        assert!(pending_exit_request_id.is_none());
        assert_eq!(
            output
                .strategy()
                .stage5g_r2ca_r2_bracket_reconcile_started_ms(),
            Some(started_ms)
        );
        let mut timer_candidate = output.strategy().stage5g_r2ca_r2_transaction_candidate();
        let admission = &output
            .recovery_receipt
            .warmup_receipt()
            .restore_receipt()
            .bootstrap_receipt()
            .admission;
        let timer_input = Stage5cPaperTimerInput {
            now_ts_utc_ms: (BAR_CLOSE_TS + 6) * 1_000,
        };
        let cleanup_ledger = stage5cj_cleanup_attribution_ledger(
            Strategy::state(&timer_candidate),
            admission.strategy_id(),
        );
        let timer_context = stage5ck_timer_context(
            &timer_candidate,
            admission,
            output.lifecycle_watermark_ts_utc,
            timer_input,
        );
        let timer_intents = crate::BrokerNeutralHybridStrategy::on_broker_timer(
            &mut timer_candidate,
            broker_core::HybridRuntimeCallbackInput {
                context: timer_context,
                payload: broker_core::HybridRuntimeTimerEvent {
                    now_ts_utc_ms: timer_input.now_ts_utc_ms,
                },
            },
        )
        .expect("timer callback preflight");
        let expected_attribution = stage5cj_expected_generated_attribution_by_request_from_ledger(
            admission,
            BAR_CLOSE_TS + 6,
            &timer_intents,
            &cleanup_ledger,
        )
        .expect("timer attribution preflight");
        let generated_request_id = stage5cg_source_request_id(
            admission.strategy_id(),
            admission.account_id().as_str(),
            &admission.target_instrument().symbol,
            BAR_CLOSE_TS + 6,
            timer_intents
                .last()
                .expect("timer must generate residual Exit"),
        )
        .expect("timer request identity");
        let pending_exit_request_id = match Strategy::state(&timer_candidate) {
            StrategyState::HybridIntradayRuntime {
                pending_exit_request_id,
                ..
            } => *pending_exit_request_id,
            StrategyState::Idle => None,
        };
        assert_eq!(
            pending_exit_request_id,
            Some(generated_request_id),
            "timer callback and Stage 5C escrow must share request identity"
        );
        let timer_batch = stage5c_build_paper_intent_batch(
            &timer_candidate,
            admission,
            BAR_CLOSE_TS + 6,
            broker_core::HybridRuntimeBarOrigin::Live,
            timer_intents,
            &expected_attribution,
        )
        .expect("timer batch preflight");
        stage5cj_verify_generated_batch_final_pending_consistency(
            Strategy::state(&timer_candidate),
            &timer_batch,
        )
        .expect("timer pending-state preflight");
        let timer = resolve_stage5c_paper_timer(output, timer_input)
            .expect("deterministic bracket timer continuation");
        assert!(timer.generated_intent_count() >= 1);
        let timer_settlement = settle_stage5c_timer_result(timer);
        let settled = timer_settlement
            .into_generated_intent_batch()
            .expect("expired bracket timer must escrow residual Exit");
        assert!(settled.intent_batch().intent_count() >= 1);
    }

    #[test]
    fn r2_canceled_or_expired_full_fill_blocks_entry_and_exit_for_both_ack_paths() {
        for ack_path in [AckPath::Accepted, AckPath::SubmittedRecovered] {
            for status in [OrderStatus::Canceled, OrderStatus::Expired] {
                let fixture = entry_fixture(ack_path);
                let evidence = terminal_evidence(
                    &fixture,
                    status.clone(),
                    fixture.order_qty,
                    fixture.order_qty,
                    3,
                );
                let blocked = validation_blocked(fixture.resolved, evidence);
                assert_eq!(
                    blocked.reason(),
                    Stage5cMarketTerminalR2Error::FullFillStatusContradiction
                );
                assert_eq!(
                    blocked.resolved().stage5g_source_intent_projections()[0].request_id,
                    fixture.request_id
                );

                let fixture = exit_fixture(ack_path, None);
                let evidence =
                    terminal_evidence(&fixture, status, fixture.order_qty, Decimal::ZERO, 3);
                let blocked = validation_blocked(fixture.resolved, evidence);
                assert_eq!(
                    blocked.reason(),
                    Stage5cMarketTerminalR2Error::FullFillStatusContradiction
                );
                assert_eq!(
                    blocked.resolved().stage5g_source_intent_projections()[0].request_id,
                    fixture.request_id
                );
            }
        }
    }

    #[test]
    fn r2_blocked_full_fill_and_timestamp_preserve_corrected_retry_capability() {
        let fixture = entry_fixture(AckPath::SubmittedRecovered);
        let request_id = fixture.request_id;
        let full = terminal_evidence(
            &fixture,
            OrderStatus::Canceled,
            fixture.order_qty,
            fixture.order_qty,
            3,
        );
        let blocked = validation_blocked(fixture.resolved, full);
        let corrected_fixture = SourceFixture {
            resolved: blocked.into_resolved(),
            request_id,
            attribution: fixture.attribution,
            intent_class: fixture.intent_class,
            side: fixture.side,
            order_qty: fixture.order_qty,
            pre_position_qty: fixture.pre_position_qty,
            bar_close_ts: fixture.bar_close_ts,
        };
        let corrected = terminal_evidence(
            &corrected_fixture,
            OrderStatus::Canceled,
            Decimal::ZERO,
            Decimal::ZERO,
            3,
        );
        let output = ready_for_timer(validate_and_settle(corrected_fixture, corrected));
        assert_eq!(
            output.resolved_batch_summary().request_ids,
            vec![request_id]
        );

        let started_ms = (BAR_CLOSE_TS + 4) * 1_000;
        let fixture = exit_fixture(AckPath::Accepted, Some(started_ms));
        let request_id = fixture.request_id;
        let stale = terminal_evidence(
            &fixture,
            OrderStatus::Expired,
            Decimal::new(4, 1),
            Decimal::new(6, 1),
            3,
        );
        let blocked = validation_blocked(fixture.resolved, stale);
        assert_eq!(
            blocked.reason(),
            Stage5cMarketTerminalR2Error::EvidenceBeforeBracketTimer
        );
        let corrected_fixture = SourceFixture {
            resolved: blocked.into_resolved(),
            request_id,
            attribution: fixture.attribution,
            intent_class: fixture.intent_class,
            side: fixture.side,
            order_qty: fixture.order_qty,
            pre_position_qty: fixture.pre_position_qty,
            bar_close_ts: fixture.bar_close_ts,
        };
        let corrected = terminal_evidence(
            &corrected_fixture,
            OrderStatus::Expired,
            Decimal::new(4, 1),
            Decimal::new(6, 1),
            5,
        );
        let output = ready_for_timer(validate_and_settle(corrected_fixture, corrected));
        assert_eq!(
            output.resolved_batch_summary().request_ids,
            vec![request_id]
        );
    }

    #[test]
    fn r2_source_owner_cycle_preflight_blocks_request_only_authority() {
        let mut fixture = entry_fixture(AckPath::Accepted);
        let mut state = Strategy::state(&fixture.resolved.strategy).clone();
        let StrategyState::HybridIntradayRuntime {
            active_cycle_id,
            pending_entry_cycle_id,
            ..
        } = &mut state
        else {
            panic!("expected hybrid runtime state")
        };
        assert!(pending_entry_cycle_id.is_some());
        *active_cycle_id = Some("deadbeef01".to_string());
        Strategy::set_state(&mut fixture.resolved.strategy, state);
        let evidence = terminal_evidence(
            &fixture,
            OrderStatus::Canceled,
            Decimal::ZERO,
            Decimal::ZERO,
            3,
        );
        let blocked = validation_blocked(fixture.resolved, evidence);
        assert_eq!(
            blocked.reason(),
            Stage5cMarketTerminalR2Error::SourceStateInconsistent
        );
        assert_eq!(
            blocked.resolved().stage5g_source_intent_projections()[0].request_id,
            fixture.request_id
        );
    }

    #[test]
    fn r2_candidate_failure_rolls_back_exact_state_and_allows_corrected_retry() {
        let fixture = entry_fixture(AckPath::Accepted);
        let original_fingerprint = fixture.resolved.post_lifecycle_state_fingerprint();
        let request_id = fixture.request_id;
        let attribution = fixture.attribution.clone();
        let intent_class = fixture.intent_class;
        let side = fixture.side;
        let order_qty = fixture.order_qty;
        let pre_position_qty = fixture.pre_position_qty;
        let bar_close_ts = fixture.bar_close_ts;
        let evidence = terminal_evidence(
            &fixture,
            OrderStatus::Canceled,
            Decimal::ONE,
            Decimal::ONE,
            3,
        );
        let mut validated = validate_stage5c_market_terminal_outcome_r2(fixture.resolved, evidence)
            .expect("valid source evidence");
        validated.source_payload =
            crate::hybrid_intraday_runtime::Stage5gR2caR2SourcePayload::Exit {
                owner: Owner::MeanReversion,
                side: Side::Long,
                cycle_id: *b"deadbeef01",
            };
        let blocked = match settle_stage5c_validated_market_terminal_outcome_r2(validated) {
            Ok(_) => panic!("fault-injected candidate unexpectedly committed"),
            Err(blocked) => blocked,
        };
        assert_eq!(
            blocked.reason(),
            Stage5cMarketTerminalR2Error::CandidatePositionFailed
        );
        assert_eq!(
            blocked.resolved().post_lifecycle_state_fingerprint(),
            original_fingerprint
        );
        let corrected_fixture = SourceFixture {
            resolved: blocked.into_resolved(),
            request_id,
            attribution,
            intent_class,
            side,
            order_qty,
            pre_position_qty,
            bar_close_ts,
        };
        let corrected = terminal_evidence(
            &corrected_fixture,
            OrderStatus::Canceled,
            Decimal::ONE,
            Decimal::ONE,
            3,
        );
        let output = generated_batch(validate_and_settle(corrected_fixture, corrected));
        assert!(output.intent_batch().intent_count() >= 1);
    }

    fn deterministic_inside_grace_fingerprint() -> (String, usize, i64) {
        let started_ms = (BAR_CLOSE_TS + 2) * 1_000;
        let fixture = exit_fixture(AckPath::Accepted, Some(started_ms));
        let evidence = terminal_evidence(
            &fixture,
            OrderStatus::Expired,
            Decimal::new(4, 1),
            Decimal::new(6, 1),
            3,
        );
        let output = ready_for_timer(validate_and_settle(fixture, evidence));
        (
            output.post_broker_lifecycle_state_fingerprint(),
            output.broker_event_count(),
            output.lifecycle_watermark_ts_utc(),
        )
    }

    #[test]
    fn r2_same_state_and_evidence_are_independent_of_process_wall_clock() {
        let first = deterministic_inside_grace_fingerprint();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let second = deterministic_inside_grace_fingerprint();
        assert_eq!(first, second);
    }
}
// STAGE5G-C-R2CA-R2-AUTHORITY-TESTS-END: deterministic-terminal-fill-boundary-v1

// STAGE5G-C-R2CA-R3-AUTHORITY-TESTS-BEGIN: exact-receipt-clock-bracket-authority-v1
#[cfg(test)]
mod stage5g_r2ca_r3_tests {
    use super::*;
    use broker_core::command::{CommandAck, CommandAckStatus};
    use broker_core::{BrokerTradeId, ClientOrderId, Exchange, Market, OrderSide, OrderStatus};
    use chrono::{Duration, TimeZone, Timelike};
    use rust_decimal::Decimal;

    use crate::hybrid_intraday::{
        HybridOrchestratorConfig, IntradayBreakoutConfig, MeanReversionConfig, Owner, Side,
    };
    use crate::hybrid_intraday_runtime::{
        HybridIntradayProfile, HybridIntradayRuntimeConfig, MeanReversionVariant, MrGatePolicy,
        RiskGateMode,
    };
    use crate::runtime_compat::{
        BarEvent, DataOrigin, MarketBuyAndCloseLiveOrderStyle, PaperExecutionMode,
    };
    use crate::{
        apply_stage5g_mock_ack, attach_stage5g_mock_ack_session, BrokerNeutralHybridIntentClass,
        BrokerNeutralOrderSide, Stage5gMockAckEvent, Stage5gMockAckSessionInput,
        Stage5gMockIntentAction, Stage5gMockIntentBinding, Stage5gMockPlaceKind,
    };

    const BAR_CLOSE_TS: i64 = 1_767_679_800;
    const BROKER_ORDER_ID: &str = "FINAM_STAGE5G_R2CA_R3_ORDER_0001";

    struct SourceFixture {
        resolved: Stage5cResolvedPaperIntentBatchStrategy,
        request_id: StrategyRequestId,
        attribution: broker_core::HybridRuntimeAttribution,
        side: BrokerNeutralOrderSide,
        order_qty: Decimal,
        bar_close_ts: i64,
    }

    fn target() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn context(close_time_utc: i64, position_qty: f64) -> StrategyCtx {
        StrategyCtx {
            strategy_id: "hybrid_imoexf".to_string(),
            portfolio: "ACC_TEST_0001".to_string(),
            exchange: "MOEX".to_string(),
            symbol: "IMOEXF".to_string(),
            tick_size: 0.5,
            trade_mode: TradeMode::Paper,
            paper_execution_mode: PaperExecutionMode::LiveOnly,
            allow_live_orders: false,
            gateway_phase: GatewayPhase::LiveReady,
            position_qty: Some(position_qty),
            event_ts_utc: close_time_utc,
            now_ts_utc: close_time_utc,
            last_bar_ts: Some(close_time_utc),
        }
    }

    fn production_exit_strategy() -> HybridIntradayRuntimeStrategy {
        let utc_bar_close = Utc.timestamp_opt(BAR_CLOSE_TS, 0).single().unwrap();
        let timezone_offset_hours = 9 - i32::try_from(utc_bar_close.hour()).unwrap();
        let local_bar_close = utc_bar_close + Duration::hours(i64::from(timezone_offset_hours));
        HybridIntradayRuntimeStrategy::new(HybridIntradayRuntimeConfig {
            symbol: "IMOEXF".to_string(),
            profile: HybridIntradayProfile::BaselineRuntimeHybrid,
            mr_variant: MeanReversionVariant::Author41BoundaryShort,
            mr_gate_policy: MrGatePolicy::Disabled,
            risk_gate_mode: RiskGateMode::Disabled,
            risk_gate_seed_file: None,
            risk_gate_ledger_key: None,
            model_session_start_time: Some((local_bar_close - Duration::minutes(10)).time()),
            model_session_end_time: Some((local_bar_close + Duration::hours(1)).time()),
            qty: 1.0,
            live_order_style: MarketBuyAndCloseLiveOrderStyle::Market,
            tick_size: 0.5,
            marketable_limit_offset_ticks: 0,
            timezone_offset_hours,
            session_close_hour: 23,
            session_close_minute: 49,
            weekends_off: false,
            stop_end_buffer_sec: 60,
            repair_deadline_sec: 180,
            sl_escalate_timeout_sec: 30,
            max_repair_retries: 3,
            repair_backoff_base_sec: 5,
            repair_backoff_max_sec: 60,
            pending_timeout_sec: 30,
            partial_entry_fill_timeout_ms: 3_000,
            mr_config: MeanReversionConfig::default(),
            breakout_config: IntradayBreakoutConfig {
                exclude_weekends: false,
                wait_hours: 0.0,
                ..IntradayBreakoutConfig::default()
            },
            orchestrator_config: HybridOrchestratorConfig::default(),
        })
    }

    fn source_exit_settled(bracket_started_ms: i64) -> Stage5cSettledPaperStrategy {
        let mut strategy = production_exit_strategy();
        for (close_time_utc, high, low) in [
            (BAR_CLOSE_TS - 86_400 - 600, 2630.0, 2570.0),
            (BAR_CLOSE_TS - 86_400, 2620.0, 2580.0),
        ] {
            assert!(Strategy::on_bar(
                &mut strategy,
                &context(close_time_utc, 0.0),
                &BarEvent {
                    symbol: "IMOEXF".to_string(),
                    close_time_utc,
                    o: 2600.0,
                    h: high,
                    l: low,
                    close: 2600.0,
                    v: 1.0,
                    origin: DataOrigin::Replay,
                },
            )
            .is_empty());
        }
        let mut state = Strategy::state(&strategy).clone();
        let StrategyState::HybridIntradayRuntime {
            active_cycle_id,
            last_position_qty,
            current_owner,
            current_side,
            ..
        } = &mut state
        else {
            panic!("R3 Exit fixture requires hybrid runtime state")
        };
        *active_cycle_id = Some("abc1230001".to_string());
        *last_position_qty = 1.0;
        *current_owner = Some(Owner::IntradayBreakout);
        *current_side = Some(Side::Long);
        Strategy::set_state(&mut strategy, state);
        let mut extension = strategy
            .stage5d_export_runtime_private_extension()
            .expect("export R3 bracket timer fixture");
        extension.bracket_reconciliation_timer = Some(
            crate::stage5d_persistence::Stage5dBracketReconciliationTimer {
                bracket_terminal_reconcile_started_ms: bracket_started_ms,
            },
        );
        strategy
            .stage5d_apply_runtime_private_extension(&extension)
            .expect("apply R3 bracket timer fixture");

        let bar = broker_core::HybridRuntimeBarEvent {
            instrument: target(),
            close_time_utc: BAR_CLOSE_TS,
            open: 2601.0,
            high: 2602.0,
            low: 2599.0,
            close: 2601.0,
            volume: 1.0,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            is_final: true,
            timeframe_sec: 600,
        };
        let lifecycle_now = Utc.timestamp_opt(BAR_CLOSE_TS - 30, 0).single().unwrap();
        let (recovered, accepted) = stage5f_test_seams::sequence_inputs_from_owned_strategy(
            strategy,
            "hybrid_imoexf".to_string(),
            BrokerAccountId::new("ACC_TEST_0001"),
            target(),
            0.5,
            Decimal::ONE,
            lifecycle_now,
            BAR_CLOSE_TS - 600,
            bar,
        );
        let semantic = apply_stage5c_semantic_bar_at(
            recovered,
            accepted,
            Utc.timestamp_opt(BAR_CLOSE_TS + 1, 0).single().unwrap(),
        )
        .expect("source-reachable Stage 5F R3 Exit callback");
        settle_stage5c_semantic_result(semantic).expect("source-reachable Stage 5F R3 escrow")
    }

    fn exit_fixture(bracket_started_ms: i64) -> SourceFixture {
        let settled = source_exit_settled(bracket_started_ms);
        let mut projections = settled.stage5g_source_intent_projections();
        assert_eq!(projections.len(), 1);
        let projection = projections.remove(0);
        assert_eq!(projection.base_action, Stage5gSourceBaseAction::Market);
        assert_eq!(
            projection.intent_class,
            BrokerNeutralHybridIntentClass::Exit
        );
        let request_id = projection.request_id;
        let side = projection.side.expect("MARKET Exit side");
        let order_qty =
            Decimal::from_f64_retain(projection.target_qty.expect("MARKET Exit target quantity"))
                .expect("exact R3 quantity");
        let attribution = projection
            .expected_attribution
            .expect("source-owned R3 attribution");
        let action = Stage5gMockIntentAction::Place {
            place_kind: Stage5gMockPlaceKind::Market,
        };
        let session = attach_stage5g_mock_ack_session(
            settled,
            Stage5gMockAckSessionInput {
                intent_bindings: vec![Stage5gMockIntentBinding {
                    request_id,
                    intent_class: BrokerNeutralHybridIntentClass::Exit,
                    action: action.clone(),
                    side: Some(side),
                }],
                lifecycle_expires_at_ts_utc: BAR_CLOSE_TS + 300,
            },
        )
        .expect("Stage 5G-b R3 source attachment");
        let resolved = apply_stage5g_mock_ack(
            session,
            Stage5gMockAckEvent {
                total_sequence: 1,
                intent_request_id: request_id,
                account_id: BrokerAccountId::new("ACC_TEST_0001"),
                instrument: target(),
                action,
                side: Some(side),
                ack: CommandAck {
                    request_id,
                    client_order_id: Some(ClientOrderId::from_strategy_request(request_id)),
                    broker_order_id: Some(BrokerOrderId::new(BROKER_ORDER_ID)),
                    status: CommandAckStatus::Accepted,
                    reason: None,
                    received_ts: Utc.timestamp_opt(BAR_CLOSE_TS + 1, 0).single().unwrap(),
                },
            },
        )
        .expect("Accepted ACK R3 source path")
        .into_resolved()
        .expect("Accepted ACK resolves R3 source");
        let (resolved, _) = resolved.into_stage5g_c_parts();
        SourceFixture {
            resolved,
            request_id,
            attribution,
            side,
            order_qty,
            bar_close_ts: BAR_CLOSE_TS,
        }
    }

    fn terminal_evidence_at(
        fixture: &SourceFixture,
        status: OrderStatus,
        filled_qty: Decimal,
        target_position_qty: Decimal,
        component_source: DateTime<Utc>,
        component_received: DateTime<Utc>,
        truth_received: DateTime<Utc>,
    ) -> Stage5cMarketTerminalOrderEvidence {
        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let broker_order_id = BrokerOrderId::new(BROKER_ORDER_ID);
        let client_order_id = ClientOrderId::from_strategy_request(fixture.request_id);
        let order_side = match fixture.side {
            BrokerNeutralOrderSide::Buy => OrderSide::Buy,
            BrokerNeutralOrderSide::Sell => OrderSide::Sell,
        };
        let order = broker_core::BrokerOrderSnapshot {
            account_id: account_id.clone(),
            broker_order_id: Some(broker_order_id.clone()),
            client_order_id: Some(client_order_id.clone()),
            instrument: target(),
            side: order_side,
            order_type: broker_core::OrderType::Market,
            time_in_force: None,
            lifecycle: broker_core::BrokerOrderSnapshot::lifecycle_for(&status),
            status,
            qty: fixture.order_qty,
            filled_qty,
            remaining_qty: Some(fixture.order_qty - filled_qty),
            limit_price: None,
            broker_asset_id: None,
            board: None,
            expiration_date: None,
            source_ts: Some(component_source),
            received_ts: component_received,
        };
        let trades = (filled_qty > Decimal::ZERO)
            .then(|| broker_core::BrokerTradeSnapshot {
                account_id: account_id.clone(),
                broker_trade_id: BrokerTradeId::new("FINAM_STAGE5G_R2CA_R3_TRADE_0001"),
                broker_order_id: Some(broker_order_id),
                client_order_id: Some(client_order_id),
                instrument: target(),
                side: order_side,
                qty: filled_qty,
                price: Decimal::new(222_750, 2),
                gross_amount: None,
                commission: None,
                broker_asset_id: None,
                board: None,
                expiration_date: None,
                source_ts: component_source,
                received_ts: component_received,
            })
            .into_iter()
            .collect();
        let positions = (target_position_qty != Decimal::ZERO)
            .then(|| broker_core::BrokerPositionSnapshot {
                account_id: account_id.clone(),
                instrument: target(),
                qty: target_position_qty,
                avg_price: Some(Decimal::new(222_750, 2)),
                unrealized_pnl: None,
                source_ts: Some(component_source),
                received_ts: component_received,
            })
            .into_iter()
            .collect();
        Stage5cMarketTerminalOrderEvidence {
            request_id: fixture.request_id,
            truth: broker_core::BrokerTruthSnapshot {
                account_id,
                orders: vec![order],
                positions,
                cash: None,
                trades,
                instruments: Vec::new(),
                received_ts: truth_received,
            },
            attribution: Some(fixture.attribution.clone()),
        }
    }

    fn partial_evidence(
        fixture: &SourceFixture,
        source: DateTime<Utc>,
        component_received: DateTime<Utc>,
        truth_received: DateTime<Utc>,
    ) -> Stage5cMarketTerminalOrderEvidence {
        terminal_evidence_at(
            fixture,
            OrderStatus::Expired,
            Decimal::new(4, 1),
            Decimal::new(6, 1),
            source,
            component_received,
            truth_received,
        )
    }

    fn ready_for_timer(
        settlement: Stage5cBrokerLifecycleSettlement,
    ) -> Stage5cBrokerLifecycleResolvedPaperStrategy {
        match settlement.inner {
            Stage5cBrokerLifecycleSettlementKind::ReadyForTimer(resolved) => resolved,
            Stage5cBrokerLifecycleSettlementKind::GeneratedIntentBatch(_)
            | Stage5cBrokerLifecycleSettlementKind::UnresolvedBrokerLifecycle(_) => {
                panic!("expected R3 ReadyForTimer")
            }
        }
    }

    fn generated_batch(
        settlement: Stage5cBrokerLifecycleSettlement,
    ) -> Stage5cSettledPaperStrategy {
        match settlement.inner {
            Stage5cBrokerLifecycleSettlementKind::GeneratedIntentBatch(settled) => settled,
            Stage5cBrokerLifecycleSettlementKind::ReadyForTimer(_)
            | Stage5cBrokerLifecycleSettlementKind::UnresolvedBrokerLifecycle(_) => {
                panic!("expected R3 GeneratedIntentBatch")
            }
        }
    }

    fn validation_blocked(
        resolved: Stage5cResolvedPaperIntentBatchStrategy,
        evidence: Stage5cMarketTerminalOrderEvidence,
    ) -> Box<Stage5cMarketTerminalR2Blocked> {
        match validate_stage5c_market_terminal_outcome_r3(resolved, evidence) {
            Ok(_) => panic!("R3 terminal evidence unexpectedly validated"),
            Err(blocked) => blocked,
        }
    }

    #[test]
    fn r3_same_second_post_start_receipt_uses_inside_grace_policy() {
        let second = Utc.timestamp_opt(BAR_CLOSE_TS + 3, 0).single().unwrap();
        let started_ms = (second + Duration::milliseconds(900)).timestamp_millis();
        let fixture = exit_fixture(started_ms);
        let evidence = partial_evidence(
            &fixture,
            second + Duration::milliseconds(920),
            second + Duration::milliseconds(930),
            second + Duration::milliseconds(950),
        );
        let validated = validate_stage5c_market_terminal_outcome_r3(fixture.resolved, evidence)
            .expect("same-second post-start receipt must validate");
        assert_eq!(
            validated.evidence_received_ms(),
            (second + Duration::milliseconds(950)).timestamp_millis()
        );
        let output = ready_for_timer(
            settle_stage5c_validated_market_terminal_outcome_r3(validated)
                .expect("inside-grace R3 settlement"),
        );
        assert_eq!(output.strategy().stage5c_current_position_qty(), 0.6);
        assert_eq!(
            output
                .strategy()
                .stage5g_r2ca_r2_bracket_reconcile_started_ms(),
            Some(started_ms)
        );
    }

    #[test]
    fn r3_pre_timer_receipt_blocks_and_preserves_capability() {
        let second = Utc.timestamp_opt(BAR_CLOSE_TS + 3, 0).single().unwrap();
        let started_ms = (second + Duration::milliseconds(900)).timestamp_millis();
        let fixture = exit_fixture(started_ms);
        let original = fixture.resolved.post_lifecycle_state_fingerprint();
        let evidence = partial_evidence(
            &fixture,
            second + Duration::milliseconds(800),
            second + Duration::milliseconds(820),
            second + Duration::milliseconds(850),
        );
        let blocked = validation_blocked(fixture.resolved, evidence);
        assert_eq!(
            blocked.reason(),
            Stage5cMarketTerminalR2Error::EvidenceBeforeBracketTimer
        );
        assert_eq!(
            blocked.resolved().post_lifecycle_state_fingerprint(),
            original
        );
    }

    #[test]
    fn r3_delayed_receipt_after_grace_escrows_recovery_immediately() {
        let second = Utc.timestamp_opt(BAR_CLOSE_TS + 3, 0).single().unwrap();
        let started_ms = second.timestamp_millis();
        let fixture = exit_fixture(started_ms);
        let evidence = partial_evidence(
            &fixture,
            second + Duration::seconds(1),
            second + Duration::milliseconds(1_100),
            second + Duration::seconds(4),
        );
        let validated = validate_stage5c_market_terminal_outcome_r3(fixture.resolved, evidence)
            .expect("delayed receipt chronology");
        let output = generated_batch(
            settle_stage5c_validated_market_terminal_outcome_r3(validated)
                .expect("post-grace R3 settlement"),
        );
        assert!(output.intent_batch().intent_count() >= 1);
    }

    #[test]
    fn r3_fresh_snapshot_same_source_later_receipt_unblocks_retry() {
        let second = Utc.timestamp_opt(BAR_CLOSE_TS + 3, 0).single().unwrap();
        let started_ms = (second + Duration::milliseconds(900)).timestamp_millis();
        let fixture = exit_fixture(started_ms);
        let request_id = fixture.request_id;
        let attribution = fixture.attribution.clone();
        let side = fixture.side;
        let order_qty = fixture.order_qty;
        let bar_close_ts = fixture.bar_close_ts;
        let source = second + Duration::milliseconds(800);
        let component_received = second + Duration::milliseconds(820);
        let stale = partial_evidence(
            &fixture,
            source,
            component_received,
            second + Duration::milliseconds(850),
        );
        let blocked = validation_blocked(fixture.resolved, stale);
        let corrected_fixture = SourceFixture {
            resolved: blocked.into_resolved(),
            request_id,
            attribution,
            side,
            order_qty,
            bar_close_ts,
        };
        let fresh = partial_evidence(
            &corrected_fixture,
            source,
            component_received,
            second + Duration::milliseconds(950),
        );
        let validated =
            validate_stage5c_market_terminal_outcome_r3(corrected_fixture.resolved, fresh)
                .expect("fresh receipt must unblock unchanged source evidence");
        ready_for_timer(
            settle_stage5c_validated_market_terminal_outcome_r3(validated)
                .expect("corrected receipt retry settlement"),
        );
    }

    fn deterministic_receipt_result() -> (String, usize, i64, i64) {
        let second = Utc.timestamp_opt(BAR_CLOSE_TS + 3, 0).single().unwrap();
        let started_ms = (second + Duration::milliseconds(900)).timestamp_millis();
        let fixture = exit_fixture(started_ms);
        let evidence = partial_evidence(
            &fixture,
            second + Duration::milliseconds(920),
            second + Duration::milliseconds(930),
            second + Duration::milliseconds(950),
        );
        let validated = validate_stage5c_market_terminal_outcome_r3(fixture.resolved, evidence)
            .expect("deterministic R3 validation");
        let receipt_ms = validated.evidence_received_ms();
        let output = ready_for_timer(
            settle_stage5c_validated_market_terminal_outcome_r3(validated)
                .expect("deterministic R3 settlement"),
        );
        (
            output.post_broker_lifecycle_state_fingerprint(),
            output.broker_event_count(),
            output.lifecycle_watermark_ts_utc(),
            receipt_ms,
        )
    }

    #[test]
    fn r3_exact_state_and_evidence_are_process_clock_independent() {
        let first = deterministic_receipt_result();
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert_eq!(first, deterministic_receipt_result());
    }

    #[test]
    fn r3_inherits_full_fill_contradiction_and_transaction_rollback() {
        let second = Utc.timestamp_opt(BAR_CLOSE_TS + 3, 0).single().unwrap();
        let started_ms = second.timestamp_millis();
        let fixture = exit_fixture(started_ms);
        let full = terminal_evidence_at(
            &fixture,
            OrderStatus::Canceled,
            fixture.order_qty,
            Decimal::ZERO,
            second + Duration::milliseconds(100),
            second + Duration::milliseconds(120),
            second + Duration::milliseconds(150),
        );
        let blocked = validation_blocked(fixture.resolved, full);
        assert_eq!(
            blocked.reason(),
            Stage5cMarketTerminalR2Error::FullFillStatusContradiction
        );

        let fixture = exit_fixture(started_ms);
        let original = fixture.resolved.post_lifecycle_state_fingerprint();
        let partial = partial_evidence(
            &fixture,
            second + Duration::milliseconds(200),
            second + Duration::milliseconds(220),
            second + Duration::milliseconds(250),
        );
        let mut validated = validate_stage5c_market_terminal_outcome_r3(fixture.resolved, partial)
            .expect("R3 rollback source validation");
        validated.validated_r2.bracket_grace_active = false;
        let blocked = settle_stage5c_validated_market_terminal_outcome_r3(validated)
            .expect_err("fault-injected R3 candidate must roll back");
        assert_eq!(
            blocked.reason(),
            Stage5cMarketTerminalR2Error::CandidateIntentPolicyMismatch
        );
        assert_eq!(
            blocked.resolved().post_lifecycle_state_fingerprint(),
            original
        );
    }
}
// STAGE5G-C-R2CA-R3-AUTHORITY-TESTS-END: exact-receipt-clock-bracket-authority-v1
