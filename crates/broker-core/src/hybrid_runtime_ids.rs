use std::collections::HashSet;

use serde::{Deserialize, Deserializer, Serialize};

use crate::ids::{
    deserialize_broker_order_id_legacy_numeric_or_string,
    deserialize_option_broker_order_id_legacy_numeric_or_string,
    deserialize_vec_broker_order_id_legacy_numeric_or_string, BrokerOrderId,
};

fn deserialize_hashset_broker_order_id_legacy_numeric_or_string<'de, D>(
    deserializer: D,
) -> Result<HashSet<BrokerOrderId>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_vec_broker_order_id_legacy_numeric_or_string(deserializer)
        .map(|values| values.into_iter().collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HybridRuntimeOwnedOrderRole {
    Entry,
    Exit,
    TakeProfit,
    StopLoss,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HybridRuntimeOwnedOrderLifecycle {
    Active,
    Terminal,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HybridRuntimeOwnedOrderUpdate {
    #[serde(deserialize_with = "deserialize_broker_order_id_legacy_numeric_or_string")]
    pub order_id: BrokerOrderId,
    pub role: HybridRuntimeOwnedOrderRole,
    pub lifecycle: HybridRuntimeOwnedOrderLifecycle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HybridRuntimeOwnedStopOrderUpdate {
    pub stop_order_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_option_broker_order_id_legacy_numeric_or_string"
    )]
    pub exchange_order_id: Option<BrokerOrderId>,
    pub role: HybridRuntimeOwnedOrderRole,
    pub lifecycle: HybridRuntimeOwnedOrderLifecycle,
    pub triggered: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HybridRuntimeOwnedIdsBootstrap {
    #[serde(default)]
    pub working_orders_strategy: Vec<HybridRuntimeOwnedOrderUpdate>,
    #[serde(default)]
    pub working_stop_orders_strategy: Vec<HybridRuntimeOwnedStopOrderUpdate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HybridRuntimeOwnedIdBlockerKind {
    FutureStopBracketOnly,
    StopOrderMissingExchangeOrderId,
    UnknownOrderLifecycle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HybridRuntimeOwnedIdBlocker {
    pub kind: HybridRuntimeOwnedIdBlockerKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_id: Option<BrokerOrderId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct HybridRuntimeOwnedIds {
    #[serde(
        default,
        deserialize_with = "deserialize_option_broker_order_id_legacy_numeric_or_string"
    )]
    pub tp_order_id: Option<BrokerOrderId>,
    #[serde(
        default,
        deserialize_with = "deserialize_option_broker_order_id_legacy_numeric_or_string"
    )]
    pub sl_exchange_order_id: Option<BrokerOrderId>,
    #[serde(
        default,
        deserialize_with = "deserialize_hashset_broker_order_id_legacy_numeric_or_string"
    )]
    pub working_orders: HashSet<BrokerOrderId>,
    #[serde(default)]
    pub blockers: Vec<HybridRuntimeOwnedIdBlocker>,
}

impl HybridRuntimeOwnedIds {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sorted_working_order_ids(&self) -> Vec<BrokerOrderId> {
        let mut ids = self.working_orders.iter().cloned().collect::<Vec<_>>();
        ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        ids
    }

    pub fn apply_order_update(&mut self, update: HybridRuntimeOwnedOrderUpdate) {
        match update.lifecycle {
            HybridRuntimeOwnedOrderLifecycle::Active => {
                self.working_orders.insert(update.order_id.clone());
                if update.role == HybridRuntimeOwnedOrderRole::TakeProfit {
                    self.tp_order_id = Some(update.order_id);
                }
            }
            HybridRuntimeOwnedOrderLifecycle::Terminal => {
                self.working_orders.remove(&update.order_id);
                if self.tp_order_id.as_ref() == Some(&update.order_id) {
                    self.tp_order_id = None;
                }
            }
            HybridRuntimeOwnedOrderLifecycle::Unknown => {
                self.blockers.push(HybridRuntimeOwnedIdBlocker {
                    kind: HybridRuntimeOwnedIdBlockerKind::UnknownOrderLifecycle,
                    order_id: Some(update.order_id),
                });
            }
        }
    }

    pub fn apply_stop_order_update(
        &mut self,
        update: HybridRuntimeOwnedStopOrderUpdate,
    ) -> Vec<BrokerOrderId> {
        let mut cancel_targets = Vec::new();
        if update.role != HybridRuntimeOwnedOrderRole::StopLoss {
            return cancel_targets;
        }

        if let Some(exchange_order_id) = update.exchange_order_id.clone() {
            self.sl_exchange_order_id = Some(exchange_order_id.clone());
            self.blockers.push(HybridRuntimeOwnedIdBlocker {
                kind: HybridRuntimeOwnedIdBlockerKind::FutureStopBracketOnly,
                order_id: Some(exchange_order_id),
            });
        } else if update.lifecycle == HybridRuntimeOwnedOrderLifecycle::Active {
            self.blockers.push(HybridRuntimeOwnedIdBlocker {
                kind: HybridRuntimeOwnedIdBlockerKind::StopOrderMissingExchangeOrderId,
                order_id: None,
            });
        }

        if update.triggered {
            if let Some(tp_order_id) = self.tp_order_id.take() {
                cancel_targets.push(tp_order_id);
            }
        }

        if update.lifecycle == HybridRuntimeOwnedOrderLifecycle::Terminal && !update.triggered {
            self.sl_exchange_order_id = None;
        }

        cancel_targets
    }

    pub fn restore_from_state(state: HybridRuntimeOwnedIds) -> Self {
        state
    }

    pub fn apply_bootstrap(&mut self, bootstrap: HybridRuntimeOwnedIdsBootstrap) {
        self.working_orders.clear();
        self.tp_order_id = None;
        self.sl_exchange_order_id = None;
        self.blockers.clear();

        for order in bootstrap.working_orders_strategy {
            self.apply_order_update(order);
        }
        for stop_order in bootstrap.working_stop_orders_strategy {
            self.apply_stop_order_update(stop_order);
        }
    }

    pub fn cancel_all_protection_targets(&mut self) -> Vec<BrokerOrderId> {
        let mut targets = Vec::new();
        if let Some(tp_order_id) = self.tp_order_id.take() {
            targets.push(tp_order_id);
        }
        if let Some(sl_exchange_order_id) = self.sl_exchange_order_id.take() {
            targets.push(sl_exchange_order_id);
        }
        targets
    }

    pub fn partial_entry_timeout_working_order_ids(&self) -> Vec<BrokerOrderId> {
        self.sorted_working_order_ids()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(
        order_id: &str,
        role: HybridRuntimeOwnedOrderRole,
        lifecycle: HybridRuntimeOwnedOrderLifecycle,
    ) -> HybridRuntimeOwnedOrderUpdate {
        HybridRuntimeOwnedOrderUpdate {
            order_id: BrokerOrderId::new(order_id),
            role,
            lifecycle,
        }
    }

    #[test]
    fn hybrid_runtime_working_orders_string_id_migration() {
        let state: HybridRuntimeOwnedIds = serde_json::from_str(
            r#"{
                "working_orders": [111, "FINAM/WORKING:EXACT_Ё"]
            }"#,
        )
        .expect("state");

        let ids = state.sorted_working_order_ids();
        let rendered = ids.iter().map(BrokerOrderId::as_str).collect::<Vec<_>>();

        assert_eq!(rendered, vec!["111", "FINAM/WORKING:EXACT_Ё"]);
    }

    #[test]
    fn hybrid_runtime_tp_order_id_string_id_migration() {
        let state: HybridRuntimeOwnedIds = serde_json::from_str(
            r#"{
                "tp_order_id": "FINAM/TP:EXACT_Ё"
            }"#,
        )
        .expect("state");

        assert_eq!(
            state.tp_order_id.as_ref().expect("tp").as_str(),
            "FINAM/TP:EXACT_Ё"
        );
    }

    #[test]
    fn hybrid_runtime_sl_exchange_order_id_string_id_migration() {
        let state: HybridRuntimeOwnedIds = serde_json::from_str(
            r#"{
                "sl_exchange_order_id": 222
            }"#,
        )
        .expect("state");

        assert_eq!(
            state.sl_exchange_order_id.as_ref().expect("sl").as_str(),
            "222"
        );
    }

    #[test]
    fn hybrid_runtime_on_order_non_empty_string_id_replaces_order_id_gt_zero() {
        let mut state = HybridRuntimeOwnedIds::new();
        state.apply_order_update(update(
            "FINAM/TP:NON_NUMERIC",
            HybridRuntimeOwnedOrderRole::TakeProfit,
            HybridRuntimeOwnedOrderLifecycle::Active,
        ));

        assert!(state
            .working_orders
            .contains(&BrokerOrderId::new("FINAM/TP:NON_NUMERIC")));
        assert_eq!(
            state.tp_order_id.as_ref().expect("tp").as_str(),
            "FINAM/TP:NON_NUMERIC"
        );

        state.apply_order_update(update(
            "FINAM/TP:NON_NUMERIC",
            HybridRuntimeOwnedOrderRole::TakeProfit,
            HybridRuntimeOwnedOrderLifecycle::Terminal,
        ));

        assert!(state.working_orders.is_empty());
        assert!(state.tp_order_id.is_none());
    }

    #[test]
    fn hybrid_runtime_bootstrap_working_orders_string_key() {
        let mut state = HybridRuntimeOwnedIds::new();
        state.apply_bootstrap(HybridRuntimeOwnedIdsBootstrap {
            working_orders_strategy: vec![update(
                "FINAM/BOOTSTRAP-TP",
                HybridRuntimeOwnedOrderRole::TakeProfit,
                HybridRuntimeOwnedOrderLifecycle::Active,
            )],
            working_stop_orders_strategy: vec![HybridRuntimeOwnedStopOrderUpdate {
                stop_order_id: Some("STOP-1".to_string()),
                exchange_order_id: Some(BrokerOrderId::new("FINAM/BOOTSTRAP-SL")),
                role: HybridRuntimeOwnedOrderRole::StopLoss,
                lifecycle: HybridRuntimeOwnedOrderLifecycle::Active,
                triggered: false,
            }],
        });

        assert_eq!(
            state.tp_order_id.as_ref().expect("tp").as_str(),
            "FINAM/BOOTSTRAP-TP"
        );
        assert_eq!(
            state.sl_exchange_order_id.as_ref().expect("sl").as_str(),
            "FINAM/BOOTSTRAP-SL"
        );
        assert!(state
            .working_orders
            .contains(&BrokerOrderId::new("FINAM/BOOTSTRAP-TP")));
    }

    #[test]
    fn hybrid_runtime_cancel_all_protection_uses_string_broker_order_id() {
        let mut state = HybridRuntimeOwnedIds {
            tp_order_id: Some(BrokerOrderId::new("FINAM/TP-CANCEL")),
            sl_exchange_order_id: Some(BrokerOrderId::new("FINAM/SL-CANCEL")),
            ..HybridRuntimeOwnedIds::default()
        };

        let targets = state.cancel_all_protection_targets();
        let rendered = targets
            .iter()
            .map(BrokerOrderId::as_str)
            .collect::<Vec<_>>();

        assert_eq!(rendered, vec!["FINAM/TP-CANCEL", "FINAM/SL-CANCEL"]);
        assert!(state.tp_order_id.is_none());
        assert!(state.sl_exchange_order_id.is_none());
    }

    #[test]
    fn hybrid_runtime_partial_entry_timeout_preserves_working_order_string_ids() {
        let mut state = HybridRuntimeOwnedIds::new();
        state.apply_order_update(update(
            "FINAM/WORK-2",
            HybridRuntimeOwnedOrderRole::Entry,
            HybridRuntimeOwnedOrderLifecycle::Active,
        ));
        state.apply_order_update(update(
            "FINAM/WORK-1",
            HybridRuntimeOwnedOrderRole::Entry,
            HybridRuntimeOwnedOrderLifecycle::Active,
        ));

        let targets = state.partial_entry_timeout_working_order_ids();
        let rendered = targets
            .iter()
            .map(BrokerOrderId::as_str)
            .collect::<Vec<_>>();

        assert_eq!(rendered, vec!["FINAM/WORK-1", "FINAM/WORK-2"]);
    }

    #[test]
    fn hybrid_runtime_stop_order_exchange_id_string_marker() {
        let mut state = HybridRuntimeOwnedIds {
            tp_order_id: Some(BrokerOrderId::new("FINAM/TP-SIBLING")),
            ..HybridRuntimeOwnedIds::default()
        };

        let cancel_targets = state.apply_stop_order_update(HybridRuntimeOwnedStopOrderUpdate {
            stop_order_id: Some("STOP-NATIVE".to_string()),
            exchange_order_id: Some(BrokerOrderId::new("FINAM/SL-EXCHANGE")),
            role: HybridRuntimeOwnedOrderRole::StopLoss,
            lifecycle: HybridRuntimeOwnedOrderLifecycle::Terminal,
            triggered: true,
        });

        assert_eq!(
            state.sl_exchange_order_id.as_ref().expect("sl").as_str(),
            "FINAM/SL-EXCHANGE"
        );
        assert_eq!(
            cancel_targets[0].as_str(),
            "FINAM/TP-SIBLING",
            "triggered SL cancels TP by exact BrokerOrderId"
        );
        assert_eq!(
            state.blockers[0].kind,
            HybridRuntimeOwnedIdBlockerKind::FutureStopBracketOnly
        );
    }

    #[test]
    fn hybrid_runtime_restored_state_preserves_string_order_ids_and_riskgate() {
        let state: HybridRuntimeOwnedIds = serde_json::from_str(
            r#"{
                "tp_order_id": 111,
                "sl_exchange_order_id": "FINAM/SL-RESTORED",
                "working_orders": ["FINAM/WORK-RESTORED", 333]
            }"#,
        )
        .expect("state");

        let restored = HybridRuntimeOwnedIds::restore_from_state(state);

        assert_eq!(restored.tp_order_id.as_ref().expect("tp").as_str(), "111");
        assert_eq!(
            restored.sl_exchange_order_id.as_ref().expect("sl").as_str(),
            "FINAM/SL-RESTORED"
        );
        assert!(restored
            .working_orders
            .contains(&BrokerOrderId::new("FINAM/WORK-RESTORED")));
        assert!(restored.working_orders.contains(&BrokerOrderId::new("333")));
    }

    #[test]
    fn hybrid_runtime_state_writes_new_string_ids() {
        let state = HybridRuntimeOwnedIds {
            tp_order_id: Some(BrokerOrderId::new("FINAM/TP-WRITE")),
            sl_exchange_order_id: Some(BrokerOrderId::new("FINAM/SL-WRITE")),
            working_orders: [BrokerOrderId::new("FINAM/WORK-WRITE")]
                .into_iter()
                .collect(),
            blockers: Vec::new(),
        };

        let value = serde_json::to_value(&state).expect("serialize");

        assert_eq!(value["tp_order_id"], "FINAM/TP-WRITE");
        assert_eq!(value["sl_exchange_order_id"], "FINAM/SL-WRITE");
        assert_eq!(value["working_orders"][0], "FINAM/WORK-WRITE");
    }
}
