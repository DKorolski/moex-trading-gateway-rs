use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{BrokerAccountId, BrokerOrderId, StrategyRequestId};
use crate::instrument::InstrumentId;
use crate::operational_snapshot::{
    BrokerOrderSnapshot, BrokerPositionSnapshot, BrokerTruthSnapshot,
};
use crate::readiness::{BrokerReadiness, ReadinessPhase, ReadinessReason};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuntimeHostLifecycleStep {
    LoadBrokerTruthSnapshot,
    LoadRuntimeState,
    NotifyBootstrapSnapshot,
    NotifyRuntimeStateRestored,
    WarmupHistory,
    RecoverPendingStreams,
}

impl RuntimeHostLifecycleStep {
    pub const ALOR_COMPATIBLE_ORDER: [RuntimeHostLifecycleStep; 6] = [
        RuntimeHostLifecycleStep::LoadBrokerTruthSnapshot,
        RuntimeHostLifecycleStep::LoadRuntimeState,
        RuntimeHostLifecycleStep::NotifyBootstrapSnapshot,
        RuntimeHostLifecycleStep::NotifyRuntimeStateRestored,
        RuntimeHostLifecycleStep::WarmupHistory,
        RuntimeHostLifecycleStep::RecoverPendingStreams,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeHostLifecyclePlan {
    pub schema_version: u16,
    pub steps: Vec<RuntimeHostLifecycleStep>,
    pub warmup_live_orders_allowed: bool,
    pub broker_truth_before_strategy_state: bool,
    pub pending_recovery_after_warmup: bool,
}

impl RuntimeHostLifecyclePlan {
    pub fn alor_compatible() -> Self {
        Self {
            schema_version: 1,
            steps: RuntimeHostLifecycleStep::ALOR_COMPATIBLE_ORDER.to_vec(),
            warmup_live_orders_allowed: false,
            broker_truth_before_strategy_state: true,
            pending_recovery_after_warmup: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeHostLifecycleIssue {
    MissingStep(RuntimeHostLifecycleStep),
    StepOutOfOrder {
        step: RuntimeHostLifecycleStep,
        expected_index: usize,
        actual_index: usize,
    },
    DuplicateStep(RuntimeHostLifecycleStep),
    WarmupAllowsLiveOrders,
    BrokerTruthNotBeforeStrategyState,
    PendingRecoveryBeforeWarmup,
}

pub fn validate_runtime_lifecycle_sequence(
    plan: &RuntimeHostLifecyclePlan,
) -> Vec<RuntimeHostLifecycleIssue> {
    let mut issues = Vec::new();

    for required in RuntimeHostLifecycleStep::ALOR_COMPATIBLE_ORDER {
        let matches: Vec<usize> = plan
            .steps
            .iter()
            .enumerate()
            .filter_map(|(idx, step)| (*step == required).then_some(idx))
            .collect();
        match matches.as_slice() {
            [] => issues.push(RuntimeHostLifecycleIssue::MissingStep(required)),
            [actual_index] => {
                let expected_index = RuntimeHostLifecycleStep::ALOR_COMPATIBLE_ORDER
                    .iter()
                    .position(|step| *step == required)
                    .expect("required step is present in canonical order");
                if *actual_index != expected_index {
                    issues.push(RuntimeHostLifecycleIssue::StepOutOfOrder {
                        step: required,
                        expected_index,
                        actual_index: *actual_index,
                    });
                }
            }
            _ => issues.push(RuntimeHostLifecycleIssue::DuplicateStep(required)),
        }
    }

    if plan.warmup_live_orders_allowed {
        issues.push(RuntimeHostLifecycleIssue::WarmupAllowsLiveOrders);
    }
    if !plan.broker_truth_before_strategy_state {
        issues.push(RuntimeHostLifecycleIssue::BrokerTruthNotBeforeStrategyState);
    }
    if !plan.pending_recovery_after_warmup {
        issues.push(RuntimeHostLifecycleIssue::PendingRecoveryBeforeWarmup);
    }

    issues
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuntimeIntentClass {
    Entry,
    Exit,
    CancelCleanup,
    ProtectiveRepair,
}

impl RuntimeIntentClass {
    pub fn is_entry(self) -> bool {
        self == RuntimeIntentClass::Entry
    }

    pub fn is_risk_reducing_or_cleanup(self) -> bool {
        matches!(
            self,
            RuntimeIntentClass::Exit
                | RuntimeIntentClass::CancelCleanup
                | RuntimeIntentClass::ProtectiveRepair
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuntimeHostBlockedIntentDisposition {
    Rollback,
    KeepStrategyState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeIntentBlockEvent {
    pub intent_class: RuntimeIntentClass,
    pub reason: String,
    pub guard_reasons: Vec<String>,
    pub disposition: RuntimeHostBlockedIntentDisposition,
    pub event_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCommandPrepared {
    pub request_id: StrategyRequestId,
    pub intent_class: RuntimeIntentClass,
    pub account_id: BrokerAccountId,
    pub instrument: InstrumentId,
    pub target_broker_order_id: Option<BrokerOrderId>,
    pub action: String,
    pub prepared_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeEventClock {
    pub last_event_ts: Option<DateTime<Utc>>,
    pub strategy_now_ts: Option<DateTime<Utc>>,
}

impl RuntimeEventClock {
    pub fn bootstrap() -> Self {
        Self {
            last_event_ts: None,
            strategy_now_ts: None,
        }
    }

    pub fn advance(&mut self, event_ts: Option<DateTime<Utc>>) -> DateTime<Utc> {
        let next = event_ts
            .or(self.strategy_now_ts)
            .or(self.last_event_ts)
            .unwrap_or_else(Utc::now);
        let monotonic = self
            .strategy_now_ts
            .map(|current| current.max(next))
            .unwrap_or(next);
        self.last_event_ts = Some(next);
        self.strategy_now_ts = Some(monotonic);
        monotonic
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeHostBootstrapSnapshot {
    pub account_id: BrokerAccountId,
    pub instrument: InstrumentId,
    pub target_position_qty: crate::instrument::Quantity,
    pub target_open_positions: Vec<BrokerPositionSnapshot>,
    pub target_active_orders: Vec<BrokerOrderSnapshot>,
    pub account_active_orders_count: usize,
    pub target_is_flat: bool,
    pub received_ts: DateTime<Utc>,
}

impl RuntimeHostBootstrapSnapshot {
    pub fn from_broker_truth(truth: &BrokerTruthSnapshot, instrument: InstrumentId) -> Self {
        let target_position_qty = truth.target_position_qty(&instrument);
        let target_open_positions = truth
            .open_positions_for_instrument(&instrument)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let target_active_orders = truth
            .target_active_orders(&instrument)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        Self {
            account_id: truth.account_id.clone(),
            instrument,
            target_position_qty,
            account_active_orders_count: truth.account_wide_active_order_count(),
            target_is_flat: target_position_qty == crate::instrument::Quantity::ZERO,
            target_open_positions,
            target_active_orders,
            received_ts: truth.received_ts,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeStrategyContext {
    pub strategy_id: String,
    pub account_id: BrokerAccountId,
    pub instrument: InstrumentId,
    pub allow_live_orders: bool,
    pub readiness: BrokerReadiness,
    pub position_qty: Option<crate::instrument::Quantity>,
    pub event_ts: DateTime<Utc>,
    pub strategy_now_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeHostContract {
    pub schema_version: u16,
    pub lifecycle_plan: RuntimeHostLifecyclePlan,
    pub intent_classes_required: bool,
    pub command_prepared_hook_required: bool,
    pub blocked_intent_rollback_required: bool,
    pub broker_truth_bootstrap_required: bool,
    pub risk_reducing_passthrough_when_blocked: bool,
}

impl RuntimeHostContract {
    pub fn alor_compatible_paper_shadow() -> Self {
        Self {
            schema_version: 1,
            lifecycle_plan: RuntimeHostLifecyclePlan::alor_compatible(),
            intent_classes_required: true,
            command_prepared_hook_required: true,
            blocked_intent_rollback_required: true,
            broker_truth_bootstrap_required: true,
            risk_reducing_passthrough_when_blocked: true,
        }
    }

    pub fn lifecycle_issues(&self) -> Vec<RuntimeHostLifecycleIssue> {
        validate_runtime_lifecycle_sequence(&self.lifecycle_plan)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeHostLiveGuardInput {
    pub allow_live_orders: bool,
    pub readiness: BrokerReadiness,
    pub readiness_stale: bool,
    pub has_seen_final_strategy_bar: bool,
    pub bars_stream_has_data: bool,
    pub operator_live_arm_present: bool,
    pub target_has_open_position: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeHostLiveGuardDecision {
    pub live_entry_allowed: bool,
    pub close_only_allowed: bool,
    pub reasons: Vec<ReadinessReason>,
}

pub fn evaluate_runtime_live_guard(
    input: RuntimeHostLiveGuardInput,
) -> RuntimeHostLiveGuardDecision {
    let mut reasons = input.readiness.reasons.clone();

    if !input.allow_live_orders {
        reasons.push(ReadinessReason::Other(
            "allow_live_orders=false".to_string(),
        ));
    }
    if input.readiness.phase != ReadinessPhase::LiveReady {
        reasons.push(ReadinessReason::Other(format!(
            "phase={:?}",
            input.readiness.phase
        )));
    }
    if input.readiness_stale {
        reasons.push(ReadinessReason::ReconciliationStale);
    }
    if !input.has_seen_final_strategy_bar {
        reasons.push(if input.bars_stream_has_data {
            ReadinessReason::Other("waiting_for_next_bar_after_restart".to_string())
        } else {
            ReadinessReason::FirstLiveBarMissing
        });
    }
    if !input.operator_live_arm_present {
        reasons.push(ReadinessReason::OperatorLiveArmMissing);
    }

    let live_entry_allowed = reasons.is_empty();
    RuntimeHostLiveGuardDecision {
        live_entry_allowed,
        close_only_allowed: !live_entry_allowed && input.target_has_open_position,
        reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    use crate::ids::BrokerAccountId;
    use crate::instrument::{Exchange, InstrumentId, Market};

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    #[test]
    fn alor_compatible_lifecycle_plan_is_valid() {
        let contract = RuntimeHostContract::alor_compatible_paper_shadow();
        assert!(contract.lifecycle_issues().is_empty());
        assert!(contract.intent_classes_required);
        assert!(contract.command_prepared_hook_required);
        assert!(contract.blocked_intent_rollback_required);
    }

    #[test]
    fn lifecycle_validator_rejects_warmup_before_broker_truth() {
        let mut plan = RuntimeHostLifecyclePlan::alor_compatible();
        plan.steps.swap(0, 4);
        plan.warmup_live_orders_allowed = true;

        let issues = validate_runtime_lifecycle_sequence(&plan);

        assert!(issues.iter().any(|issue| matches!(
            issue,
            RuntimeHostLifecycleIssue::StepOutOfOrder {
                step: RuntimeHostLifecycleStep::LoadBrokerTruthSnapshot,
                ..
            }
        )));
        assert!(issues
            .iter()
            .any(|issue| matches!(issue, RuntimeHostLifecycleIssue::WarmupAllowsLiveOrders)));
    }

    #[test]
    fn bootstrap_snapshot_is_target_symbol_scoped() {
        let now = Utc::now();
        let target = instrument();
        let other = InstrumentId {
            symbol: "USDRUBF".to_string(),
            venue_symbol: Some("USDRUBF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        };
        let truth = BrokerTruthSnapshot {
            account_id: BrokerAccountId("ACC_TEST_0001".to_string()),
            orders: Vec::new(),
            positions: vec![
                BrokerPositionSnapshot {
                    account_id: BrokerAccountId("ACC_TEST_0001".to_string()),
                    instrument: target.clone(),
                    qty: Decimal::new(-3, 0),
                    avg_price: None,
                    unrealized_pnl: None,
                    source_ts: Some(now),
                    received_ts: now,
                },
                BrokerPositionSnapshot {
                    account_id: BrokerAccountId("ACC_TEST_0001".to_string()),
                    instrument: other,
                    qty: Decimal::new(-1, 0),
                    avg_price: None,
                    unrealized_pnl: None,
                    source_ts: Some(now),
                    received_ts: now,
                },
            ],
            cash: None,
            trades: Vec::new(),
            instruments: Vec::new(),
            received_ts: now,
        };

        let snapshot = RuntimeHostBootstrapSnapshot::from_broker_truth(&truth, target);

        assert_eq!(snapshot.target_position_qty, Decimal::new(-3, 0));
        assert_eq!(snapshot.target_open_positions.len(), 1);
        assert!(!snapshot.target_is_flat);
        assert_eq!(snapshot.account_active_orders_count, 0);
    }

    #[test]
    fn live_guard_keeps_close_only_path_when_blocked_with_open_position() {
        let now = Utc::now();
        let decision = evaluate_runtime_live_guard(RuntimeHostLiveGuardInput {
            allow_live_orders: true,
            readiness: BrokerReadiness {
                phase: ReadinessPhase::Reconciliation,
                reasons: vec![ReadinessReason::OperatorLiveArmMissing],
                checked_ts: now,
            },
            readiness_stale: false,
            has_seen_final_strategy_bar: true,
            bars_stream_has_data: true,
            operator_live_arm_present: false,
            target_has_open_position: true,
        });

        assert!(!decision.live_entry_allowed);
        assert!(decision.close_only_allowed);
        assert!(decision
            .reasons
            .contains(&ReadinessReason::OperatorLiveArmMissing));
    }

    #[test]
    fn event_clock_is_monotonic_for_out_of_order_events() {
        let mut clock = RuntimeEventClock::bootstrap();
        let later = DateTime::parse_from_rfc3339("2026-07-06T10:10:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let earlier = DateTime::parse_from_rfc3339("2026-07-06T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        assert_eq!(clock.advance(Some(later)), later);
        assert_eq!(clock.advance(Some(earlier)), later);
        assert_eq!(clock.last_event_ts, Some(earlier));
        assert_eq!(clock.strategy_now_ts, Some(later));
    }
}
