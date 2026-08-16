use super::*;
use crate::stage8a4_reconciliation::{
    Stage8a4DurableRequestContext, Stage8a4PrivateAccountSafety, Stage8a4ReconciliationReason,
    Stage8a4SourceTiming,
};
use broker_core::{
    BrokerAccountId, CancelOrder, ClientOrderId, Exchange, HybridRuntimeAttribution, InstrumentId,
    Market, OrderSide, OrderStatus, OrderType, PlaceOrder, StrategyRequestId, TimeInForce,
};
use chrono::{Duration, TimeZone, Utc};
use rust_decimal::Decimal;
use strategy_runtime_core::{
    Stage6DurableCommandSnapshotV1, Stage6JournalRecordV1, Stage6ReconciliationTransitionKindV2,
};
use uuid::Uuid;

const FP: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const FP2: &str = "2222222222222222222222222222222222222222222222222222222222222222";
const FP3: &str = "3333333333333333333333333333333333333333333333333333333333333333";

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 16, 9, 0, 0).unwrap()
}

fn request(value: u128) -> StrategyRequestId {
    StrategyRequestId::from(Uuid::from_u128(value))
}

fn account() -> BrokerAccountId {
    BrokerAccountId::new("ACC_TEST_0001")
}

fn instrument() -> InstrumentId {
    InstrumentId {
        symbol: "IMOEXF".into(),
        venue_symbol: Some("IMOEXF@RTSX".into()),
        exchange: Exchange::Moex,
        market: Market::Futures,
    }
}

fn attribution(role: &str) -> HybridRuntimeAttribution {
    HybridRuntimeAttribution::parse_source_comment(format!(
        "HYB|sid=hybrid_imoexf|c=cycle0001|o=BO|r={role}"
    ))
    .unwrap()
}

fn place_identity() -> (Stage6DurableRequestIdentityV1, Stage6JournalRecordV1) {
    let request_id = request(1);
    let command = PlaceOrder {
        request_id,
        created_ts: now(),
        ttl_ms: Some(5_000),
        account_id: account(),
        client_order_id: ClientOrderId::from_strategy_request(request_id),
        instrument: instrument(),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        qty: Decimal::from(2),
        limit_price: Some(Decimal::from(2210)),
        time_in_force: TimeInForce::Day,
        comment: Some(attribution("ENTRY").internal_comment().to_string()),
    };
    let identity =
        Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap();
    let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, &command).unwrap();
    let accepted = Stage6JournalRecordV1::request_accepted(
        identity.clone(),
        snapshot,
        Stage6LifecycleSequence::new(1).unwrap(),
        None,
        None,
        digest(FP),
    )
    .unwrap();
    (identity, accepted)
}

fn cancel_identity() -> (Stage6DurableRequestIdentityV1, Stage6JournalRecordV1) {
    let request_id = request(2);
    let command = CancelOrder {
        request_id,
        created_ts: now(),
        ttl_ms: Some(5_000),
        account_id: account(),
        order_id: BrokerOrderId::new("BROKER-ORDER-1"),
        client_order_id: Some(ClientOrderId::from_strategy_request(request(1))),
    };
    let identity =
        Stage6DurableRequestIdentityV1::from_cancel(&command, instrument(), attribution("CANCEL"))
            .unwrap();
    let snapshot = Stage6DurableCommandSnapshotV1::from_cancel(&identity, &command).unwrap();
    let accepted = Stage6JournalRecordV1::request_accepted(
        identity.clone(),
        snapshot,
        Stage6LifecycleSequence::new(1).unwrap(),
        None,
        None,
        digest(FP),
    )
    .unwrap();
    (identity, accepted)
}

fn order(status: OrderStatus, broker_order_id: Option<BrokerOrderId>) -> BrokerOrderSnapshot {
    let filled_qty = if status == OrderStatus::Filled {
        Decimal::from(2)
    } else {
        Decimal::ZERO
    };
    BrokerOrderSnapshot {
        account_id: account(),
        broker_order_id,
        client_order_id: Some(ClientOrderId::from_strategy_request(request(1))),
        instrument: instrument(),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        time_in_force: Some(TimeInForce::Day),
        lifecycle: BrokerOrderSnapshot::lifecycle_for(&status),
        status,
        qty: Decimal::from(2),
        filled_qty,
        remaining_qty: Some(Decimal::from(2) - filled_qty),
        limit_price: Some(Decimal::from(2210)),
        broker_asset_id: Some("ASSET_IMOEXF".into()),
        board: Some("RFUD".into()),
        expiration_date: None,
        source_ts: Some(now() - Duration::seconds(2)),
        received_ts: now() - Duration::seconds(1),
    }
}

fn trade() -> BrokerTradeSnapshot {
    BrokerTradeSnapshot {
        account_id: account(),
        broker_trade_id: broker_core::BrokerTradeId::new("TRADE-1"),
        broker_order_id: Some(BrokerOrderId::new("BROKER-ORDER-1")),
        client_order_id: Some(ClientOrderId::from_strategy_request(request(1))),
        instrument: instrument(),
        side: OrderSide::Buy,
        qty: Decimal::from(2),
        price: Decimal::from(2210),
        gross_amount: None,
        commission: None,
        broker_asset_id: Some("ASSET_IMOEXF".into()),
        board: Some("RFUD".into()),
        expiration_date: None,
        source_ts: now() - Duration::seconds(2),
        received_ts: now() - Duration::seconds(1),
    }
}

fn exact_outcome(
    identity: &Stage6DurableRequestIdentityV1,
    lifecycle: Stage8a4ExactLifecycle,
    selected: BrokerOrderSnapshot,
    trades: Vec<BrokerTradeSnapshot>,
) -> Stage8a4AuthoritativeReconciliationOutcome {
    let fill = match lifecycle {
        Stage8a4ExactLifecycle::TerminalFilled => Stage8a4FillEffect::Full {
            filled_qty: Decimal::from(2),
        },
        _ => Stage8a4FillEffect::Zero,
    };
    let context = Stage8a4DurableRequestContext {
        request_id: identity.strategy_request_id(),
        client_order_id: identity.durable_client_order_id().clone(),
        account_id: identity.account_id().clone(),
        instrument: identity.instrument().clone(),
        side: OrderSide::Buy,
        qty: Decimal::from(2),
        order_type: OrderType::Limit,
        time_in_force: TimeInForce::Day,
        limit_price: Some(Decimal::from(2210)),
        known_broker_order_id: selected.broker_order_id.clone(),
        possible_effect_at: now() - Duration::seconds(10),
        event_start: now() - Duration::seconds(9),
        event_end: now() - Duration::seconds(1),
        durable_binding_sha256: FP.into(),
    };
    Stage8a4AuthoritativeReconciliationOutcome {
        context,
        outcome_kind: Stage8a4OutcomeKind::ExactOrderState,
        reason: Stage8a4ReconciliationReason::ExactTier1ClientIdentity,
        lifecycle: Some(lifecycle),
        fill: Some(fill),
        selected_order_binding_sha256: Some(FP.into()),
        trade_summary_binding_sha256: Some(FP.into()),
        matching_trade_count: trades.len(),
        semantic_binding_sha256: FP.into(),
        selected_order: Some(selected),
        material_trades: trades,
        exact_lookup: Stage8a4PrivateExactLookup::NotAttempted,
        account_safety: Stage8a4PrivateAccountSafety {
            account_active_orders_count: 0,
            account_unknown_orders_count: 0,
            account_orphan_orders_count: 0,
            account_open_positions_count: 0,
            target_active_orders_count: 0,
            target_unknown_orders_count: 0,
            target_terminal_orders_count: 1,
            target_inconsistent_orders_count: 0,
            target_open_positions_count: 0,
            other_symbol_active_orders_count: 0,
        },
        source_evidence_binding_sha256: FP.into(),
        private_outcome_binding_sha256: FP2.into(),
    }
}

fn input(
    identity: Stage6DurableRequestIdentityV1,
    accepted: &Stage6JournalRecordV1,
    outcome: Stage8a4AuthoritativeReconciliationOutcome,
    seal_generation: u64,
) -> Stage8a4I2CompositionInput {
    Stage8a4I2CompositionInput {
        identity,
        cursor: PrivateJournalCursor {
            previous_record_id: accepted.journal_record_id().clone(),
            previous_lifecycle_sequence: accepted.lifecycle_sequence(),
        },
        pre_append: PrivatePreAppendEvidence {
            expected_stage6_checkpoint_or_frontier_fingerprint: digest(FP),
            expected_recovery_seal_generation: seal_generation,
            expected_recovery_seal_fingerprint: digest(FP2),
            expected_request_state_fingerprint: digest(FP3),
        },
        outcome,
    }
}

fn digest(value: &str) -> Stage6Sha256Digest {
    Stage6Sha256Digest::parse(value).unwrap()
}

#[test]
fn place_exact_filled_builds_v2_then_lossless_v1_suffix() {
    let (identity, accepted) = place_identity();
    let outcome = exact_outcome(
        &identity,
        Stage8a4ExactLifecycle::TerminalFilled,
        order(
            OrderStatus::Filled,
            Some(BrokerOrderId::new("BROKER-ORDER-1")),
        ),
        vec![trade()],
    );
    let candidate =
        build_private_durable_candidate(input(identity, &accepted, outcome, 7)).unwrap();
    assert_eq!(candidate.transition_record.lifecycle_sequence().get(), 2);
    assert_eq!(candidate.suffix_records.len(), 3);
    assert_eq!(candidate.suffix_records[0].lifecycle_sequence().get(), 3);
    assert_eq!(candidate.suffix_records[2].lifecycle_sequence().get(), 5);
    assert_eq!(
        candidate
            .transition_record
            .payload()
            .suffix_manifest()
            .entries()
            .len(),
        3
    );
    assert!(matches!(
        candidate.transition_record.payload().transition_kind(),
        Stage6ReconciliationTransitionKindV2::Exact { .. }
    ));
}

#[test]
fn stable_key_ignores_mutable_preappend_generation() {
    let (identity, accepted) = place_identity();
    let make = |generation| {
        let outcome = exact_outcome(
            &identity,
            Stage8a4ExactLifecycle::Working,
            order(
                OrderStatus::Working,
                Some(BrokerOrderId::new("BROKER-ORDER-1")),
            ),
            vec![],
        );
        build_private_durable_candidate(input(identity.clone(), &accepted, outcome, generation))
            .unwrap()
    };
    let first = make(7);
    let second = make(8);
    assert_eq!(
        first
            .transition_record
            .payload()
            .stable_transition_key_sha256(),
        second
            .transition_record
            .payload()
            .stable_transition_key_sha256()
    );
    assert_ne!(
        first.transition_record.encode_canonical(),
        second.transition_record.encode_canonical()
    );
}

#[test]
fn place_without_broker_id_never_fabricates_order_or_trade_suffix() {
    let (identity, accepted) = place_identity();
    let outcome = exact_outcome(
        &identity,
        Stage8a4ExactLifecycle::Working,
        order(OrderStatus::Working, None),
        vec![],
    );
    let candidate =
        build_private_durable_candidate(input(identity, &accepted, outcome, 1)).unwrap();
    assert!(candidate
        .transition_record
        .payload()
        .broker_order_fact()
        .unwrap()
        .broker_order_id()
        .is_none());
    assert_eq!(candidate.suffix_records.len(), 1);
}

#[test]
fn cancel_working_remains_unresolved_without_suffix() {
    let (identity, accepted) = cancel_identity();
    let outcome = exact_outcome(
        &identity,
        Stage8a4ExactLifecycle::Working,
        order(
            OrderStatus::Working,
            Some(BrokerOrderId::new("BROKER-ORDER-1")),
        ),
        vec![],
    );
    let candidate =
        build_private_durable_candidate(input(identity, &accepted, outcome, 1)).unwrap();
    assert!(candidate.suffix_records.is_empty());
}

#[test]
fn cancel_terminal_cancelled_projects_outcome_and_finalization_only() {
    let (identity, accepted) = cancel_identity();
    let outcome = exact_outcome(
        &identity,
        Stage8a4ExactLifecycle::TerminalCancelled,
        order(
            OrderStatus::Canceled,
            Some(BrokerOrderId::new("BROKER-ORDER-1")),
        ),
        vec![],
    );
    let candidate =
        build_private_durable_candidate(input(identity, &accepted, outcome, 1)).unwrap();
    assert_eq!(candidate.suffix_records.len(), 2);
    assert_eq!(candidate.suffix_records[0].lifecycle_sequence().get(), 3);
    assert_eq!(candidate.suffix_records[1].lifecycle_sequence().get(), 4);
}

#[test]
fn succeeded_exact_lookup_preserves_typed_observation() {
    let (identity, accepted) = place_identity();
    let selected = order(
        OrderStatus::Working,
        Some(BrokerOrderId::new("BROKER-ORDER-1")),
    );
    let mut outcome = exact_outcome(
        &identity,
        Stage8a4ExactLifecycle::Working,
        selected.clone(),
        vec![],
    );
    outcome.exact_lookup = Stage8a4PrivateExactLookup::Succeeded(Box::new(
        super::super::Stage8a4ExactOrderObservation {
            order: selected,
            timing: Stage8a4SourceTiming {
                request_started_at: now() - Duration::seconds(2),
                response_received_at: now() - Duration::seconds(1),
            },
        },
    ));
    let candidate =
        build_private_durable_candidate(input(identity, &accepted, outcome, 1)).unwrap();
    assert!(matches!(
        candidate.transition_record.payload().transition_kind(),
        Stage6ReconciliationTransitionKindV2::Exact { .. }
    ));
}

#[test]
fn documented_not_found_without_exact_contradiction_is_still_unknown() {
    let (identity, accepted) = place_identity();
    let selected = order(
        OrderStatus::Working,
        Some(BrokerOrderId::new("BROKER-ORDER-1")),
    );
    let mut outcome = exact_outcome(&identity, Stage8a4ExactLifecycle::Working, selected, vec![]);
    outcome.selected_order = None;
    outcome.outcome_kind = Stage8a4OutcomeKind::StillUnknown;
    outcome.lifecycle = None;
    outcome.fill = None;
    outcome.exact_lookup = Stage8a4PrivateExactLookup::DocumentedNotFound {
        timing: Stage8a4SourceTiming {
            request_started_at: now() - Duration::seconds(2),
            response_received_at: now() - Duration::seconds(1),
        },
        documented_status_category: "http_404_documented".into(),
    };
    let candidate =
        build_private_durable_candidate(input(identity, &accepted, outcome, 1)).unwrap();
    assert!(matches!(
        candidate.transition_record.payload().transition_kind(),
        Stage6ReconciliationTransitionKindV2::ReconciliationStillUnknownHold
    ));
    assert!(candidate.suffix_records.is_empty());
}

#[test]
fn non_success_lookup_states_never_become_exact() {
    let states = ["not_found", "unavailable", "decode", "stale"];
    for state in states {
        let (identity, accepted) = place_identity();
        let mut outcome = exact_outcome(
            &identity,
            Stage8a4ExactLifecycle::Working,
            order(
                OrderStatus::Working,
                Some(BrokerOrderId::new("BROKER-ORDER-1")),
            ),
            vec![],
        );
        let timing = Stage8a4SourceTiming {
            request_started_at: now() - Duration::seconds(2),
            response_received_at: now() - Duration::seconds(1),
        };
        outcome.exact_lookup = match state {
            "not_found" => Stage8a4PrivateExactLookup::DocumentedNotFound {
                timing,
                documented_status_category: "http_404_documented".into(),
            },
            "unavailable" => Stage8a4PrivateExactLookup::Unavailable {
                timing,
                failure_category: "timeout".into(),
            },
            "decode" => Stage8a4PrivateExactLookup::DecodeFailure {
                timing,
                response_status_category: "http_200".into(),
                response_binding_sha256: FP3.into(),
            },
            "stale" => Stage8a4PrivateExactLookup::Stale {
                timing,
                stale_observation_binding_sha256: FP3.into(),
            },
            _ => unreachable!(),
        };
        let candidate =
            build_private_durable_candidate(input(identity, &accepted, outcome, 1)).unwrap();
        assert!(!matches!(
            candidate.transition_record.payload().transition_kind(),
            Stage6ReconciliationTransitionKindV2::Exact { .. }
        ));
        assert!(candidate.suffix_records.is_empty());
    }
}

#[test]
fn cancel_target_cross_binding_is_mandatory() {
    let (identity, accepted) = cancel_identity();
    let mut outcome = exact_outcome(
        &identity,
        Stage8a4ExactLifecycle::TerminalCancelled,
        order(
            OrderStatus::Canceled,
            Some(BrokerOrderId::new("BROKER-ORDER-1")),
        ),
        vec![],
    );
    outcome.context.known_broker_order_id = Some(BrokerOrderId::new("OTHER"));
    let error = match build_private_durable_candidate(input(identity, &accepted, outcome, 1)) {
        Ok(_) => panic!("cancel target mismatch must fail closed"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        Stage8a4I2CompositionError::IdentityMismatch
    ));
}

#[test]
fn candidate_is_pure_and_deterministic_for_identical_inputs() {
    let build = || {
        let (identity, accepted) = place_identity();
        let outcome = exact_outcome(
            &identity,
            Stage8a4ExactLifecycle::TerminalFilled,
            order(
                OrderStatus::Filled,
                Some(BrokerOrderId::new("BROKER-ORDER-1")),
            ),
            vec![trade()],
        );
        build_private_durable_candidate(input(identity, &accepted, outcome, 3)).unwrap()
    };
    let first = build();
    let second = build();
    assert_eq!(
        first.transition_record.encode_canonical(),
        second.transition_record.encode_canonical()
    );
    assert_eq!(
        first
            .suffix_records
            .iter()
            .map(Stage6JournalRecordV1::encode_canonical)
            .collect::<Vec<_>>(),
        second
            .suffix_records
            .iter()
            .map(Stage6JournalRecordV1::encode_canonical)
            .collect::<Vec<_>>()
    );
}
