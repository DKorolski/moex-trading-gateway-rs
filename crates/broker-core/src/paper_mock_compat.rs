//! Stage 2B paper/mock compatibility contracts.
//!
//! This module intentionally has no broker transport and no live-order path. It
//! groups the broker-neutral runtime migration invariants that must hold
//! together before the real strategy runtime source is migrated.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage2bPaperMockCompatibilityReport {
    pub schema_version: u16,
    pub old_state_roundtrip_preserved: bool,
    pub ack_exact_request_id_policy_preserved: bool,
    pub broker_order_id_string_paths_preserved: bool,
    pub ownership_attribution_safe: bool,
    pub deterministic_request_id_stable: bool,
    pub riskgate_seed_preserved: bool,
    pub live_boundary_closed: bool,
}

impl Stage2bPaperMockCompatibilityReport {
    pub fn accepted(&self) -> bool {
        self.schema_version == 1
            && self.old_state_roundtrip_preserved
            && self.ack_exact_request_id_policy_preserved
            && self.broker_order_id_string_paths_preserved
            && self.ownership_attribution_safe
            && self.deterministic_request_id_stable
            && self.riskgate_seed_preserved
            && self.live_boundary_closed
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use rust_decimal::Decimal;
    use serde_json::json;

    use super::Stage2bPaperMockCompatibilityReport;
    use crate::account::AccountId;
    use crate::command::{
        build_cancel_command, BrokerCommand, CancelOrderBuilderInput, CommandAck,
        CommandAckReasonCode, CommandAckStatus, ReplaceOrder,
    };
    use crate::hybrid_runtime_ids::HybridRuntimeOwnedIds;
    use crate::ids::{BrokerOrderId, BrokerTradeId, ClientOrderId, StrategyRequestId};
    use crate::instrument::{Exchange, InstrumentId, Market};
    use crate::order::{OrderSide, OrderType, TimeInForce};
    use crate::paper::{
        PaperExecutionMode, PaperHybridIntradayOracleSeed, PaperLedgerExecutorConfig,
        PaperLedgerSnapshot,
    };
    use crate::request_id::{
        deterministic_request_id_for_account_instrument, deterministic_request_id_from_legacy_parts,
    };
    use crate::runtime_state::{
        RuntimeAckLifecycleIssue, RuntimeAckPendingDisposition, RuntimeAckStatusPolicy,
        RuntimeCacheApplyDisposition, RuntimeCacheLifecycleBlocker, RuntimeCaches,
        RuntimeOrderAttribution, RuntimeOrderEvent, RuntimeOrderEventLifecycle, RuntimePendingPath,
        RuntimePendingRequestIdentity, RuntimeStateSnapshot, RuntimeTradeCacheTarget,
        RuntimeTradeEvent,
    };
    use crate::trade_ledger::{
        OrderRecord, TradeLedger, TradeLedgerBlockerKind, TradeLedgerFillDisposition, TradeRecord,
    };

    fn ts() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 9, 9, 10, 0)
            .single()
            .expect("valid timestamp")
    }

    fn account() -> AccountId {
        AccountId::new("ACC_TEST_ALIAS")
    }

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn pending_entry_request_id() -> StrategyRequestId {
        deterministic_request_id_from_legacy_parts(
            "hybrid_imoexf",
            "ACC_TEST_ALIAS",
            "IMOEXF",
            "place",
            1_783_456_200,
            3,
        )
    }

    fn pending_exit_request_id() -> StrategyRequestId {
        deterministic_request_id_from_legacy_parts(
            "hybrid_imoexf",
            "ACC_TEST_ALIAS",
            "IMOEXF",
            "exit",
            1_783_456_800,
            4,
        )
    }

    fn ack(
        request_id: StrategyRequestId,
        broker_order_id: Option<&str>,
        status: CommandAckStatus,
    ) -> CommandAck {
        CommandAck {
            request_id,
            client_order_id: Some(ClientOrderId::new("CID_STAGE2B10_001").expect("valid cid")),
            broker_order_id: broker_order_id.map(BrokerOrderId::new),
            status,
            reason: None,
            received_ts: ts(),
        }
    }

    fn runtime_order(order_id: &str, status: &str) -> RuntimeOrderEvent {
        RuntimeOrderEvent {
            order_id: BrokerOrderId::new(order_id),
            client_order_id: None,
            symbol: Some("IMOEXF".to_string()),
            exchange: Some("MOEX".to_string()),
            status: Some(status.to_string()),
            side: Some("buy".to_string()),
            order_type: Some("limit".to_string()),
            source_ts: Some(ts()),
        }
    }

    fn runtime_trade(trade_id: &str, order_id: &str) -> RuntimeTradeEvent {
        RuntimeTradeEvent {
            trade_id: BrokerTradeId::new(trade_id),
            order_id: BrokerOrderId::new(order_id),
            client_order_id: None,
            symbol: Some("IMOEXF".to_string()),
            exchange: Some("MOEX".to_string()),
            side: Some("buy".to_string()),
            source_ts: Some(ts()),
        }
    }

    fn ledger_order(order_id: &str, owned: bool) -> OrderRecord {
        OrderRecord {
            order_id: BrokerOrderId::new(order_id),
            symbol: "IMOEXF".to_string(),
            side: "buy".to_string(),
            qty: 1.0,
            filled: 1.0,
            price: 2210.0,
            status: "filled".to_string(),
            ts_utc: ts().timestamp(),
            owned,
        }
    }

    fn ledger_trade(trade_id: &str, order_id: &str) -> TradeRecord {
        TradeRecord {
            ts_utc: ts().timestamp(),
            trade_id: Some(BrokerTradeId::new(trade_id)),
            order_id: BrokerOrderId::new(order_id),
            symbol: "IMOEXF".to_string(),
            side: "buy".to_string(),
            qty: 1.0,
            price: 2210.0,
            commission: 0.0,
            owned: false,
        }
    }

    #[test]
    fn stage_2b10_combined_paper_mock_compatibility_pack_preserves_contracts() {
        let entry_request_id = pending_entry_request_id();
        let exit_request_id = pending_exit_request_id();

        let old_alor_state = json!({
            "schema_version": 1,
            "orders": {
                "101": {
                    "order_id": 101,
                    "status": "working",
                    "symbol": "IMOEXF",
                    "side": "buy",
                    "order_type": "limit"
                },
                "FINAM/OBSERVED-2B10": {
                    "order_id": "FINAM/OBSERVED-2B10",
                    "status": "working",
                    "symbol": "IMOEXF"
                }
            },
            "known_order_ids": [101],
            "trades": [{
                "trade_id": "ALOR-TRADE-LEGACY-2B10",
                "order_id": 101,
                "symbol": "IMOEXF",
                "side": "buy"
            }],
            "pending_entry_request_id": entry_request_id,
            "pending_exit_request_id": exit_request_id,
            "deferred_entry_state": "waiting_next_bar",
            "deferred_exit_state": "armed_after_closed_bar",
            "manual_intervention_required": true,
            "manual_intervention_reason": "synthetic_stage_2b10"
        });

        let restored = serde_json::from_value::<RuntimeStateSnapshot>(old_alor_state)
            .expect("old ALOR-style state imports")
            .validate_for_runtime_restore()
            .expect("old ALOR-style state validates")
            .snapshot;
        let serialized_v2 = serde_json::to_string(&restored).expect("v2 state serializes");
        let restored_again = serde_json::from_str::<RuntimeStateSnapshot>(&serialized_v2)
            .expect("v2 state restores")
            .validate_for_runtime_restore()
            .expect("v2 restored state validates")
            .snapshot;

        assert_eq!(restored_again.schema_version, 2);
        assert_eq!(
            restored_again
                .orders
                .get(&BrokerOrderId::new("101"))
                .expect("legacy order preserved")
                .order_id
                .as_str(),
            "101"
        );
        assert_eq!(
            restored_again
                .known_order_ids
                .iter()
                .map(BrokerOrderId::as_str)
                .collect::<Vec<_>>(),
            vec!["101"]
        );
        assert_eq!(
            restored_again.pending_entry_request_id,
            Some(entry_request_id)
        );
        assert_eq!(
            restored_again.pending_exit_request_id,
            Some(exit_request_id)
        );
        assert_eq!(
            restored_again.deferred_entry_state.as_deref(),
            Some("waiting_next_bar")
        );
        assert_eq!(
            restored_again.deferred_exit_state.as_deref(),
            Some("armed_after_closed_bar")
        );
        assert!(restored_again.manual_intervention_required);
        assert_eq!(
            restored_again.manual_intervention_reason.as_deref(),
            Some("synthetic_stage_2b10")
        );

        let mut caches = RuntimeCaches::from_validated_state(
            restored_again
                .clone()
                .validate_for_runtime_restore()
                .expect("state remains valid"),
        );

        assert_eq!(
            caches
                .tracked_order_ids()
                .iter()
                .map(BrokerOrderId::as_str)
                .collect::<Vec<_>>(),
            vec!["101"]
        );
        assert!(caches
            .observed_order_ids
            .contains(&BrokerOrderId::new("FINAM/OBSERVED-2B10")));
        assert!(!caches
            .owned_order_ids
            .contains(&BrokerOrderId::new("FINAM/OBSERVED-2B10")));

        let exact_ack = ack(
            entry_request_id,
            Some("FINAM/ENTRY-ACCEPTED-2B10"),
            CommandAckStatus::Submitted,
        );
        let decision = caches
            .apply_ack_to_pending_path(RuntimePendingPath::Entry, &exact_ack)
            .expect("pending entry evaluated");
        assert_eq!(
            decision.pending_disposition,
            RuntimeAckPendingDisposition::ClearPending
        );
        assert!(caches.pending_entry.is_none());

        let mismatched_ack = ack(
            deterministic_request_id_from_legacy_parts(
                "hybrid_imoexf",
                "ACC_TEST_ALIAS",
                "IMOEXF",
                "exit",
                1_783_456_800,
                9,
            ),
            Some("FINAM/EXIT-2B10"),
            CommandAckStatus::Rejected,
        );
        let decision = caches
            .apply_ack_to_pending_path(RuntimePendingPath::Exit, &mismatched_ack)
            .expect("pending exit evaluated");
        assert_eq!(
            decision.pending_disposition,
            RuntimeAckPendingDisposition::KeepPending
        );
        assert_eq!(
            decision.issues,
            vec![RuntimeAckLifecycleIssue::RequestIdMismatch]
        );
        assert!(caches.pending_exit.is_some());

        for status in [
            CommandAckStatus::Error,
            CommandAckStatus::Duplicate,
            CommandAckStatus::Expired,
        ] {
            let decision = caches
                .apply_ack_to_pending_path(
                    RuntimePendingPath::Exit,
                    &ack(exit_request_id, None, status),
                )
                .expect("pending exit evaluated");
            assert_eq!(
                decision.pending_disposition,
                RuntimeAckPendingDisposition::KeepPending
            );
            assert!(caches.pending_exit.is_some());
        }
        assert_eq!(
            caches
                .pending_exit
                .as_ref()
                .expect("exit pending preserved")
                .request_id,
            exit_request_id
        );

        let pending_trade_outcome = caches.apply_trade_event(runtime_trade(
            "FINAM/TRADE-PENDING-2B10",
            "FINAM/OWNED-2B10",
        ));
        assert_eq!(
            pending_trade_outcome.target,
            RuntimeTradeCacheTarget::PendingOrderEvent
        );
        let owned_order_outcome =
            caches.apply_owned_order_event(runtime_order("FINAM/OWNED-2B10", "filled"));
        assert_eq!(owned_order_outcome.reconciled_pending_trade_count, 1);
        assert_eq!(
            owned_order_outcome.lifecycle,
            RuntimeOrderEventLifecycle::Terminal
        );
        assert_eq!(
            caches
                .trades_by_order_id
                .get(&BrokerOrderId::new("FINAM/OWNED-2B10"))
                .expect("owned trade reconciled")
                .len(),
            1
        );

        let observed_trade_outcome = caches.apply_trade_event(runtime_trade(
            "FINAM/TRADE-OBSERVED-2B10",
            "FINAM/OBSERVED-2B10",
        ));
        assert_eq!(
            observed_trade_outcome.target,
            RuntimeTradeCacheTarget::KnownOrder
        );
        assert!(!caches
            .owned_order_ids
            .contains(&BrokerOrderId::new("FINAM/OBSERVED-2B10")));

        let orphan_outcome = caches.apply_order_event_with_attribution(
            runtime_order("FINAM/ORPHAN-2B10", "working"),
            RuntimeOrderAttribution::UnknownOrOrphan,
        );
        assert_eq!(
            orphan_outcome.lifecycle_blocker,
            Some(RuntimeCacheLifecycleBlocker::UnknownOrOrphanOwnership)
        );
        assert_eq!(
            orphan_outcome.disposition,
            RuntimeCacheApplyDisposition::Inserted
        );
        assert!(!caches
            .tracked_order_ids()
            .contains(&BrokerOrderId::new("FINAM/ORPHAN-2B10")));

        let mut ledger = TradeLedger::default();
        assert_eq!(
            ledger
                .record_fill(ledger_trade(
                    "FINAM/TRADE-LEDGER-PENDING-2B10",
                    "FINAM/LEDGER-OWNED-2B10"
                ))
                .disposition,
            TradeLedgerFillDisposition::PendingExactBrokerOrderMatch
        );
        assert_eq!(
            ledger
                .record_order(ledger_order("FINAM/LEDGER-OBSERVED-2B10", false))
                .adopted_pending_strategy_trades,
            0
        );
        assert_eq!(ledger.pending_trades_total(), 1);
        assert_eq!(
            ledger
                .record_order(ledger_order("FINAM/LEDGER-OWNED-2B10", true))
                .adopted_pending_strategy_trades,
            1
        );
        assert_eq!(ledger.pending_trades_total(), 0);
        assert_eq!(ledger.trades().len(), 1);
        assert_eq!(
            ledger
                .record_fill(ledger_trade(
                    "FINAM/TRADE-OBSERVED-2B10",
                    "FINAM/LEDGER-OBSERVED-2B10"
                ))
                .disposition,
            TradeLedgerFillDisposition::ObservedOnly
        );
        assert_eq!(
            ledger.active_blockers()[0].kind,
            TradeLedgerBlockerKind::ObservedOrderNotStrategyOwned
        );

        let cancel_command = build_cancel_command(CancelOrderBuilderInput {
            request_id: entry_request_id,
            created_ts: ts(),
            ttl_ms: Some(1_000),
            account_id: account(),
            order_id: BrokerOrderId::new("FINAM/CANCEL-2B10"),
            client_order_id: None,
        });
        match cancel_command {
            BrokerCommand::CancelOrder(cancel) => {
                assert_eq!(cancel.order_id.as_str(), "FINAM/CANCEL-2B10");
            }
            BrokerCommand::PlaceOrder(_) => panic!("cancel builder returned place order"),
        }

        let replace: ReplaceOrder = serde_json::from_value(json!({
            "request_id": exit_request_id,
            "created_ts": ts(),
            "ttl_ms": null,
            "account_id": "ACC_TEST_ALIAS",
            "order_id": "FINAM/REPLACE-2B10",
            "client_order_id": null,
            "new_qty": "2",
            "new_limit_price": "2210.5"
        }))
        .expect("replace DTO preserves broker order id");
        assert_eq!(replace.order_id.as_str(), "FINAM/REPLACE-2B10");
        assert_eq!(
            replace.feature_disabled().reason,
            CommandAckReasonCode::FeatureDisabled
        );

        let place = crate::command::PlaceOrder {
            request_id: entry_request_id,
            created_ts: ts(),
            ttl_ms: Some(1_000),
            account_id: account(),
            client_order_id: ClientOrderId::new("CID_STAGE2B10_002").expect("valid cid"),
            instrument: instrument(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            qty: Decimal::ONE,
            limit_price: Some(Decimal::new(2_2105, 1)),
            time_in_force: TimeInForce::Day,
            comment: Some("stage2b10-paper-mock-no-send".to_string()),
        };
        assert_eq!(place.request_id, entry_request_id);
        assert_eq!(place.instrument.symbol, "IMOEXF");

        let legacy_request_id = deterministic_request_id_from_legacy_parts(
            "hybrid_imoexf",
            "ACC_TEST_ALIAS",
            "IMOEXF",
            "place",
            1_783_456_200,
            3,
        );
        let broker_neutral_request_id = deterministic_request_id_for_account_instrument(
            "hybrid_imoexf",
            &account(),
            &instrument(),
            "place",
            1_783_456_200,
            3,
        );
        assert_eq!(legacy_request_id, broker_neutral_request_id);
        assert_eq!(legacy_request_id, entry_request_id);

        let hybrid_ids: HybridRuntimeOwnedIds = serde_json::from_value(json!({
            "tp_order_id": 202,
            "sl_exchange_order_id": "FINAM/SL-2B10",
            "working_orders": [202, "FINAM/SL-2B10", "FINAM/WORK-2B10"]
        }))
        .expect("hybrid owned ids import legacy numeric and string ids");
        let restored_hybrid_ids = HybridRuntimeOwnedIds::restore_from_state(hybrid_ids);
        assert_eq!(
            restored_hybrid_ids
                .tp_order_id
                .as_ref()
                .map(BrokerOrderId::as_str),
            Some("202")
        );
        assert!(restored_hybrid_ids
            .working_orders
            .contains(&BrokerOrderId::new("FINAM/WORK-2B10")));

        let mut paper_snapshot = PaperLedgerSnapshot::empty(
            PaperLedgerExecutorConfig::new(
                "hybrid_imoexf",
                instrument(),
                PaperExecutionMode::HistorySim,
                600,
            ),
            ts(),
        );
        paper_snapshot
            .apply_hybrid_intraday_oracle_seed(
                PaperHybridIntradayOracleSeed {
                    source: "stage_2b10_fixture".to_string(),
                    active_cycle_id: Some("cycle-stage-2b10".to_string()),
                    next_cycle_seq: Some(42),
                    last_position_qty: Some(Decimal::new(-3, 0)),
                    current_owner: Some("intraday_breakout".to_string()),
                    current_side: Some("short".to_string()),
                    pending_entry_request_id: Some(entry_request_id.to_string()),
                    pending_exit_request_id: Some(exit_request_id.to_string()),
                    deferred_exit_state: Some("armed_after_closed_bar".to_string()),
                    manual_intervention_required: Some(true),
                    manual_intervention_reason: Some("synthetic_stage_2b10".to_string()),
                    risk_gate_profile_id: Some("synthetic_profile".to_string()),
                    risk_gate_shadow_session_date: Some("2026-07-09".to_string()),
                    risk_gate_shadow_pnl_points: Some(Decimal::new(125, 1)),
                    risk_gate_shadow_trade_count: Some(3),
                    risk_gate_mr_enabled_current_session: Some(true),
                    risk_gate_mr_enabled_next_session: Some(false),
                    risk_gate_rolling_sum_lb120: Some(Decimal::new(1_586, 1)),
                    risk_gate_last_finalized_session_date: Some("2026-07-08".to_string()),
                    risk_gate_ledger_rows_count: Some(222),
                    ..PaperHybridIntradayOracleSeed::default()
                },
                ts(),
            )
            .expect("paper oracle seed preserves riskgate/runtime fields");
        let projection = paper_snapshot.to_hybrid_intraday_runtime_state_projection();
        assert_eq!(
            projection.active_cycle_id.as_deref(),
            Some("cycle-stage-2b10")
        );
        assert_eq!(
            projection.pending_entry_request_id,
            Some(entry_request_id.to_string())
        );
        assert_eq!(
            projection.pending_exit_request_id,
            Some(exit_request_id.to_string())
        );
        assert_eq!(
            projection.deferred_exit_state.as_deref(),
            Some("armed_after_closed_bar")
        );
        assert!(projection.manual_intervention_required);
        assert_eq!(
            projection.risk_gate_shadow_session_date.as_deref(),
            Some("2026-07-09")
        );
        assert_eq!(projection.risk_gate_shadow_trade_count, 3);
        assert_eq!(projection.risk_gate_mr_enabled_current_session, Some(true));
        assert_eq!(projection.risk_gate_mr_enabled_next_session, Some(false));
        assert_eq!(projection.risk_gate_rolling_sum_lb120, Some(158.6));
        assert_eq!(
            projection.risk_gate_last_finalized_session_date.as_deref(),
            Some("2026-07-08")
        );
        assert_eq!(projection.risk_gate_ledger_rows_count, 222);
        assert!(!projection.strategy_invocation_enabled);
        assert!(paper_snapshot.safety_boundary.is_closed());

        let report = Stage2bPaperMockCompatibilityReport {
            schema_version: 1,
            old_state_roundtrip_preserved: restored_again.pending_exit_request_id
                == Some(exit_request_id),
            ack_exact_request_id_policy_preserved: caches.pending_entry.is_none()
                && caches.pending_exit.is_some(),
            broker_order_id_string_paths_preserved: ledger
                .order(&BrokerOrderId::new("FINAM/LEDGER-OWNED-2B10"))
                .is_some(),
            ownership_attribution_safe: !caches
                .tracked_order_ids()
                .contains(&BrokerOrderId::new("FINAM/OBSERVED-2B10"))
                && !caches
                    .tracked_order_ids()
                    .contains(&BrokerOrderId::new("FINAM/ORPHAN-2B10")),
            deterministic_request_id_stable: legacy_request_id == broker_neutral_request_id,
            riskgate_seed_preserved: projection.risk_gate_ledger_rows_count == 222,
            live_boundary_closed: paper_snapshot.safety_boundary.is_closed(),
        };
        assert!(report.accepted());

        let report_json = serde_json::to_value(&report).expect("report serializes");
        assert_eq!(report_json["live_boundary_closed"], true);
    }

    #[test]
    fn stage_2b10_expired_ack_with_no_send_proof_is_the_only_expired_clear_path() {
        let request_id = pending_entry_request_id();
        let mut caches = RuntimeCaches::new();
        caches.set_pending(
            RuntimePendingPath::Entry,
            RuntimePendingRequestIdentity {
                request_id,
                client_order_id: None,
                broker_order_id: None,
            },
        );

        let ambiguous_expired = ack(request_id, None, CommandAckStatus::Expired);
        let decision = caches
            .apply_ack_to_pending_path(RuntimePendingPath::Entry, &ambiguous_expired)
            .expect("pending exists");
        assert_eq!(
            decision.status_policy,
            RuntimeAckStatusPolicy::RequiresNoSendProof
        );
        assert_eq!(
            decision.pending_disposition,
            RuntimeAckPendingDisposition::KeepPending
        );
        assert!(caches.pending_entry.is_some());

        let no_send_expired = CommandAck {
            reason: Some(crate::command::CommandAckReason::new(
                CommandAckReasonCode::ExpiredCommand,
            )),
            ..ambiguous_expired
        };
        let decision = caches
            .apply_ack_to_pending_path(RuntimePendingPath::Entry, &no_send_expired)
            .expect("pending exists");
        assert_eq!(decision.status_policy, RuntimeAckStatusPolicy::ClearPending);
        assert_eq!(
            decision.pending_disposition,
            RuntimeAckPendingDisposition::ClearPending
        );
        assert!(caches.pending_entry.is_none());
    }
}
