use super::*;
use crate::stage8a4_reconciliation::{
    admit_stage8a4_broker_truth, canonical_truth_binding, interval_coverage_fingerprint,
    private_authoritative_outcome_binding, reduce_stage8a4_authoritative,
    Stage8a4BoundedTradeHistoryComplete, Stage8a4CompletePositionsSnapshot,
    Stage8a4DurableRequestContext, Stage8a4FreshTruthAdmission,
    Stage8a4InstrumentCompletenessEvidence, Stage8a4NonPaginatedOrdersSnapshotComplete,
    Stage8a4PrivateAccountSafety, Stage8a4ReconciliationPolicy, Stage8a4ReconciliationReason,
    Stage8a4SourceEvidence, Stage8a4SourceTiming, Stage8a4TradeIntervalProof,
};
use broker_core::{
    BrokerAccountId, BrokerInstrumentSpec, BrokerKind, BrokerSymbol, BrokerTruthSnapshot,
    CancelOrder, ClientOrderId, Exchange, HybridRuntimeAttribution, InstrumentId,
    InstrumentMapEntry, InternalSymbol, Market, OrderSide, OrderStatus, OrderType, PlaceOrder,
    StrategyRequestId, TimeInForce,
};
use chrono::{Duration, TimeZone, Utc};
use rust_decimal::Decimal;
use strategy_runtime_core::{
    Stage6DurableCommandSnapshotV1, Stage6JournalRecordV1, Stage6JournalRecordVersioned,
    Stage6MixedReplayEngineV2, Stage6ReconciliationTransitionKindV2,
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

fn instrument_spec() -> BrokerInstrumentSpec {
    BrokerInstrumentSpec {
        instrument: InstrumentMapEntry {
            internal_symbol: InternalSymbol("IMOEXF".into()),
            broker: BrokerKind::Finam,
            broker_symbol: BrokerSymbol("IMOEXF@RTSX".into()),
            exchange: Exchange::Moex,
            market: Market::Futures,
            price_step: Decimal::new(5, 1),
            qty_step: Decimal::ONE,
            lot_size: Decimal::ONE,
            min_qty: Decimal::ONE,
            step_value: Decimal::ONE,
            currency: "RUB".into(),
            schedule_id: "MOEX_FUT".into(),
            expiration_date: None,
            is_tradable: true,
        },
        broker_asset_id: Some("ASSET_IMOEXF".into()),
        board: Some("RFUD".into()),
        long_initial_margin: None,
        short_initial_margin: None,
    }
}

fn attribution(role: &str) -> HybridRuntimeAttribution {
    attribution_for("cycle0001", role)
}

fn attribution_for(cycle: &str, role: &str) -> HybridRuntimeAttribution {
    HybridRuntimeAttribution::parse_source_comment(format!(
        "HYB|sid=hybrid_imoexf|c={cycle}|o=BO|r={role}"
    ))
    .unwrap()
}

fn place_command(request_id: StrategyRequestId) -> PlaceOrder {
    PlaceOrder {
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
    }
}

fn accepted_place(
    command: &PlaceOrder,
    command_attribution: HybridRuntimeAttribution,
) -> (Stage6DurableRequestIdentityV1, Stage6JournalRecordV1) {
    let identity =
        Stage6DurableRequestIdentityV1::from_place(command, command_attribution).unwrap();
    let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, command).unwrap();
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

fn place_identity() -> (Stage6DurableRequestIdentityV1, Stage6JournalRecordV1) {
    accepted_place(&place_command(request(1)), attribution("ENTRY"))
}

fn cancel_command(request_id: StrategyRequestId) -> CancelOrder {
    CancelOrder {
        request_id,
        created_ts: now(),
        ttl_ms: Some(5_000),
        account_id: account(),
        order_id: BrokerOrderId::new("BROKER-ORDER-1"),
        client_order_id: Some(ClientOrderId::from_strategy_request(request(1))),
    }
}

fn accepted_cancel(
    command: &CancelOrder,
    command_attribution: HybridRuntimeAttribution,
) -> (Stage6DurableRequestIdentityV1, Stage6JournalRecordV1) {
    let identity =
        Stage6DurableRequestIdentityV1::from_cancel(command, instrument(), command_attribution)
            .unwrap();
    let snapshot = Stage6DurableCommandSnapshotV1::from_cancel(&identity, command).unwrap();
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
    accepted_cancel(&cancel_command(request(2)), attribution("CANCEL"))
}

fn dispatch_attempt(
    identity: &Stage6DurableRequestIdentityV1,
    accepted: &Stage6JournalRecordV1,
) -> Stage6JournalRecordV1 {
    Stage6JournalRecordV1::dispatch_attempt_recorded(
        identity.clone(),
        1,
        accepted.canonical_payload_sha256().clone(),
        Stage6LifecycleSequence::new(2).unwrap(),
        Some(accepted.journal_record_id().clone()),
        digest(FP2),
    )
    .unwrap()
}

fn mixed_replay(
    accepted: Stage6JournalRecordV1,
    dispatch: Stage6JournalRecordV1,
    candidate: Stage8a4I2DurableCandidate,
) -> strategy_runtime_core::Stage6MixedReplaySnapshotV2 {
    let mut records = vec![
        Stage6JournalRecordVersioned::V1(accepted),
        Stage6JournalRecordVersioned::V1(dispatch),
        Stage6JournalRecordVersioned::V2(candidate.transition_record),
    ];
    records.extend(
        candidate
            .suffix_records
            .into_iter()
            .map(Stage6JournalRecordVersioned::V1),
    );
    Stage6MixedReplayEngineV2::replay(&records).unwrap()
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

fn durable_context(
    identity: &Stage6DurableRequestIdentityV1,
    accepted: &Stage6JournalRecordV1,
    known_broker_order_id: Option<BrokerOrderId>,
) -> Stage8a4DurableRequestContext {
    Stage8a4DurableRequestContext {
        request_id: identity.strategy_request_id(),
        client_order_id: identity.durable_client_order_id().clone(),
        account_id: identity.account_id().clone(),
        instrument: identity.instrument().clone(),
        action: identity.action(),
        attribution: identity.attribution().clone(),
        side: OrderSide::Buy,
        qty: Decimal::from(2),
        order_type: OrderType::Limit,
        time_in_force: TimeInForce::Day,
        limit_price: Some(Decimal::from(2210)),
        known_broker_order_id,
        target_order_client_order_id: identity.target_order_client_order_id().cloned(),
        accepted_command_payload_sha256: accepted.canonical_payload_sha256().as_str().to_string(),
        possible_effect_at: now() - Duration::seconds(10),
        event_start: now() - Duration::seconds(9),
        event_end: now() - Duration::seconds(1),
        durable_binding_sha256: FP.into(),
    }
}

fn reconciliation_policy() -> Stage8a4ReconciliationPolicy {
    Stage8a4ReconciliationPolicy {
        trusted_now: now(),
        max_source_age: Duration::minutes(2),
        max_cross_source_skew: Duration::seconds(30),
        max_trade_intervals: 8,
        max_interval_split_depth: 4,
        policy_binding_sha256: FP.into(),
    }
}

fn source_timing() -> Stage8a4SourceTiming {
    Stage8a4SourceTiming {
        request_started_at: now() - Duration::seconds(2),
        response_received_at: now() - Duration::seconds(1),
    }
}

fn broker_truth(
    orders: Vec<BrokerOrderSnapshot>,
    trades: Vec<BrokerTradeSnapshot>,
    instruments: Vec<BrokerInstrumentSpec>,
) -> BrokerTruthSnapshot {
    BrokerTruthSnapshot {
        account_id: account(),
        orders,
        positions: vec![],
        cash: None,
        trades,
        instruments,
        received_ts: now() - Duration::seconds(1),
    }
}

fn source_evidence(
    context: &Stage8a4DurableRequestContext,
    truth: &BrokerTruthSnapshot,
    exact_lookup: Stage8a4PrivateExactLookup,
) -> Stage8a4SourceEvidence {
    let intervals = vec![Stage8a4TradeIntervalProof {
        start_inclusive: context.event_start,
        end_exclusive: context.event_end,
        requested_limit: 100,
        returned_count: truth.trades.len(),
        request_started_at: now() - Duration::seconds(2),
        response_received_at: now() - Duration::seconds(1),
        split_depth: 0,
    }];
    let interval_refs = intervals.iter().collect::<Vec<_>>();
    Stage8a4SourceEvidence {
        orders: Stage8a4NonPaginatedOrdersSnapshotComplete {
            timing: source_timing(),
        },
        trades: Stage8a4BoundedTradeHistoryComplete {
            interval_coverage_sha256: interval_coverage_fingerprint(&interval_refs),
            intervals,
        },
        positions: Stage8a4CompletePositionsSnapshot {
            timing: source_timing(),
        },
        instruments: Stage8a4InstrumentCompletenessEvidence::ExactTargetResolved {
            timing: source_timing(),
        },
        exact_lookup,
        canonical_truth_payload_sha256: canonical_truth_binding(truth),
        acquisition_policy_sha256: FP.into(),
    }
}

fn production_outcome(
    identity: &Stage6DurableRequestIdentityV1,
    accepted: &Stage6JournalRecordV1,
    truth: BrokerTruthSnapshot,
    exact_lookup: Stage8a4PrivateExactLookup,
) -> Stage8a4AuthoritativeReconciliationOutcome {
    let context = durable_context(
        identity,
        accepted,
        Some(BrokerOrderId::new("BROKER-ORDER-1")),
    );
    let evidence = source_evidence(&context, &truth, exact_lookup);
    let policy = reconciliation_policy();
    let admission = admit_stage8a4_broker_truth(&context, &policy, truth, evidence).unwrap();
    reduce_stage8a4_authoritative(context, admission, policy)
}

fn fresh_writer_admission(
    identity: &Stage6DurableRequestIdentityV1,
    accepted: &Stage6JournalRecordV1,
    truth: BrokerTruthSnapshot,
) -> Stage8a4FreshTruthAdmission {
    let context = durable_context(
        identity,
        accepted,
        Some(BrokerOrderId::new("BROKER-ORDER-1")),
    );
    let evidence = source_evidence(&context, &truth, Stage8a4PrivateExactLookup::NotAttempted);
    admit_stage8a4_broker_truth(&context, &reconciliation_policy(), truth, evidence).unwrap()
}

fn exact_outcome(
    identity: &Stage6DurableRequestIdentityV1,
    accepted: &Stage6JournalRecordV1,
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
    let context = durable_context(identity, accepted, selected.broker_order_id.clone());
    let exact_lookup = Stage8a4PrivateExactLookup::NotAttempted;
    let account_safety = Stage8a4PrivateAccountSafety {
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
    };
    let private_outcome_binding_sha256 =
        private_authoritative_outcome_binding(&context, FP, FP, &exact_lookup, &account_safety);
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
        exact_lookup,
        account_safety,
        source_evidence_binding_sha256: FP.into(),
        private_outcome_binding_sha256,
    }
}

fn rebind_outcome_to_accepted(
    outcome: &mut Stage8a4AuthoritativeReconciliationOutcome,
    accepted: &Stage6JournalRecordV1,
) {
    outcome.context.accepted_command_payload_sha256 =
        accepted.canonical_payload_sha256().as_str().to_string();
    outcome.private_outcome_binding_sha256 = private_authoritative_outcome_binding(
        &outcome.context,
        &outcome.semantic_binding_sha256,
        &outcome.source_evidence_binding_sha256,
        &outcome.exact_lookup,
        &outcome.account_safety,
    );
}

fn input(
    accepted: &Stage6JournalRecordV1,
    cursor: &Stage6JournalRecordV1,
    outcome: Stage8a4AuthoritativeReconciliationOutcome,
    seal_generation: u64,
) -> Stage8a4I2CompositionInput {
    Stage8a4I2CompositionInput {
        accepted_command_authority: accepted_command_authority_from_request_accepted(accepted)
            .unwrap(),
        cursor: PrivateJournalCursor {
            previous_record_id: cursor.journal_record_id().clone(),
            previous_lifecycle_sequence: cursor.lifecycle_sequence(),
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
        &accepted,
        Stage8a4ExactLifecycle::TerminalFilled,
        order(
            OrderStatus::Filled,
            Some(BrokerOrderId::new("BROKER-ORDER-1")),
        ),
        vec![trade()],
    );
    let candidate =
        build_private_durable_candidate(input(&accepted, &accepted, outcome, 7)).unwrap();
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
            &accepted,
            Stage8a4ExactLifecycle::Working,
            order(
                OrderStatus::Working,
                Some(BrokerOrderId::new("BROKER-ORDER-1")),
            ),
            vec![],
        );
        build_private_durable_candidate(input(&accepted, &accepted, outcome, generation)).unwrap()
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
        &accepted,
        Stage8a4ExactLifecycle::Working,
        order(OrderStatus::Working, None),
        vec![],
    );
    let candidate =
        build_private_durable_candidate(input(&accepted, &accepted, outcome, 1)).unwrap();
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
        &accepted,
        Stage8a4ExactLifecycle::Working,
        order(
            OrderStatus::Working,
            Some(BrokerOrderId::new("BROKER-ORDER-1")),
        ),
        vec![],
    );
    let candidate =
        build_private_durable_candidate(input(&accepted, &accepted, outcome, 1)).unwrap();
    assert!(candidate.suffix_records.is_empty());
}

#[test]
fn cancel_terminal_cancelled_projects_outcome_and_finalization_only() {
    let (identity, accepted) = cancel_identity();
    let outcome = exact_outcome(
        &identity,
        &accepted,
        Stage8a4ExactLifecycle::TerminalCancelled,
        order(
            OrderStatus::Canceled,
            Some(BrokerOrderId::new("BROKER-ORDER-1")),
        ),
        vec![],
    );
    let candidate =
        build_private_durable_candidate(input(&accepted, &accepted, outcome, 1)).unwrap();
    assert_eq!(candidate.suffix_records.len(), 2);
    assert_eq!(candidate.suffix_records[0].lifecycle_sequence().get(), 3);
    assert_eq!(candidate.suffix_records[1].lifecycle_sequence().get(), 4);
}

#[test]
fn all_six_exact_lookup_states_traverse_source_admission_and_owner() {
    for state in [
        "not_attempted",
        "succeeded",
        "documented_not_found",
        "unavailable",
        "decode_failure",
        "stale",
    ] {
        let (identity, accepted) = place_identity();
        let selected = order(
            OrderStatus::Working,
            Some(BrokerOrderId::new("BROKER-ORDER-1")),
        );
        let exact_lookup = match state {
            "not_attempted" => Stage8a4PrivateExactLookup::NotAttempted,
            "succeeded" => Stage8a4PrivateExactLookup::Succeeded(Box::new(
                super::super::Stage8a4ExactOrderObservation {
                    order: selected.clone(),
                    timing: source_timing(),
                },
            )),
            "documented_not_found" => Stage8a4PrivateExactLookup::DocumentedNotFound {
                timing: source_timing(),
                documented_status_category: "http_404_documented".into(),
            },
            "unavailable" => Stage8a4PrivateExactLookup::Unavailable {
                timing: source_timing(),
                failure_category: "timeout".into(),
            },
            "decode_failure" => Stage8a4PrivateExactLookup::DecodeFailure {
                timing: source_timing(),
                response_status_category: "http_200".into(),
                response_binding_sha256: FP3.into(),
            },
            "stale" => Stage8a4PrivateExactLookup::Stale {
                timing: source_timing(),
                stale_observation_binding_sha256: FP3.into(),
            },
            _ => unreachable!(),
        };
        let outcome = production_outcome(
            &identity,
            &accepted,
            broker_truth(vec![selected], vec![], vec![instrument_spec()]),
            exact_lookup,
        );
        let candidate =
            build_private_durable_candidate(input(&accepted, &accepted, outcome, 1)).unwrap();
        let encoded: serde_json::Value =
            serde_json::from_slice(&candidate.transition_record.encode_canonical()).unwrap();
        assert_eq!(encoded["payload"]["exact_lookup_evidence"]["state"], state);
        let transition = candidate.transition_record.payload().transition_kind();
        match state {
            "not_attempted" | "succeeded" => assert!(matches!(
                transition,
                Stage6ReconciliationTransitionKindV2::Exact { .. }
            )),
            "documented_not_found" => assert!(matches!(
                transition,
                Stage6ReconciliationTransitionKindV2::ReconciliationConflictHold
            )),
            "unavailable" | "decode_failure" | "stale" => assert!(matches!(
                transition,
                Stage6ReconciliationTransitionKindV2::ReconciliationStillUnknownHold
            )),
            _ => unreachable!(),
        }
    }
}

#[test]
fn documented_not_found_without_source_contradiction_is_still_unknown() {
    let (identity, accepted) = place_identity();
    let outcome = production_outcome(
        &identity,
        &accepted,
        broker_truth(vec![], vec![], vec![instrument_spec()]),
        Stage8a4PrivateExactLookup::DocumentedNotFound {
            timing: source_timing(),
            documented_status_category: "http_404_documented".into(),
        },
    );
    let candidate =
        build_private_durable_candidate(input(&accepted, &accepted, outcome, 1)).unwrap();
    assert!(matches!(
        candidate.transition_record.payload().transition_kind(),
        Stage6ReconciliationTransitionKindV2::ReconciliationStillUnknownHold
    ));
    assert!(candidate.suffix_records.is_empty());
}

#[test]
fn cancel_disposition_table_preserves_predecessor_semantics() {
    for (lifecycle, status, expected) in [
        (
            Stage8a4ExactLifecycle::TerminalFilled,
            OrderStatus::Filled,
            Stage6CancelOutcomeV1::ExecutionObserved,
        ),
        (
            Stage8a4ExactLifecycle::TerminalRejected,
            OrderStatus::Rejected,
            Stage6CancelOutcomeV1::AlreadyTerminalNonExecution,
        ),
        (
            Stage8a4ExactLifecycle::TerminalCancelled,
            OrderStatus::Canceled,
            Stage6CancelOutcomeV1::Canceled,
        ),
        (
            Stage8a4ExactLifecycle::TerminalExpired,
            OrderStatus::Expired,
            Stage6CancelOutcomeV1::AlreadyTerminalNonExecution,
        ),
    ] {
        let (identity, accepted) = cancel_identity();
        let dispatch = dispatch_attempt(&identity, &accepted);
        let trades = if lifecycle == Stage8a4ExactLifecycle::TerminalFilled {
            vec![trade()]
        } else {
            vec![]
        };
        let outcome = exact_outcome(
            &identity,
            &accepted,
            lifecycle,
            order(status, Some(BrokerOrderId::new("BROKER-ORDER-1"))),
            trades,
        );
        let candidate =
            build_private_durable_candidate(input(&accepted, &dispatch, outcome, 1)).unwrap();
        assert_eq!(candidate.suffix_records.len(), 2);
        let replay = mixed_replay(accepted, dispatch, candidate);
        let recovered = replay.requests().first().unwrap();
        assert_eq!(recovered.cancel_outcome(), Some(expected));
        assert_eq!(
            recovered.final_disposition(),
            Some(Stage6RequestFinalDispositionV1::Completed)
        );
    }

    let (identity, accepted) = cancel_identity();
    let dispatch = dispatch_attempt(&identity, &accepted);
    let working = exact_outcome(
        &identity,
        &accepted,
        Stage8a4ExactLifecycle::Working,
        order(
            OrderStatus::Working,
            Some(BrokerOrderId::new("BROKER-ORDER-1")),
        ),
        vec![],
    );
    let candidate =
        build_private_durable_candidate(input(&accepted, &dispatch, working, 1)).unwrap();
    assert!(candidate.suffix_records.is_empty());

    for outcome_kind in [
        Stage8a4OutcomeKind::Conflict,
        Stage8a4OutcomeKind::StillUnknown,
    ] {
        let mut hold = exact_outcome(
            &identity,
            &accepted,
            Stage8a4ExactLifecycle::Working,
            order(
                OrderStatus::Working,
                Some(BrokerOrderId::new("BROKER-ORDER-1")),
            ),
            vec![],
        );
        hold.outcome_kind = outcome_kind;
        hold.lifecycle = None;
        hold.fill = None;
        hold.private_outcome_binding_sha256 = private_authoritative_outcome_binding(
            &hold.context,
            &hold.semantic_binding_sha256,
            &hold.source_evidence_binding_sha256,
            &hold.exact_lookup,
            &hold.account_safety,
        );
        let candidate =
            build_private_durable_candidate(input(&accepted, &dispatch, hold, 1)).unwrap();
        assert!(candidate.suffix_records.is_empty());
    }
}

#[test]
fn material_trade_broker_id_projects_when_selected_order_id_is_missing() {
    let (identity, accepted) = place_identity();
    let dispatch = dispatch_attempt(&identity, &accepted);
    let outcome = exact_outcome(
        &identity,
        &accepted,
        Stage8a4ExactLifecycle::TerminalFilled,
        order(OrderStatus::Filled, None),
        vec![trade()],
    );
    let candidate =
        build_private_durable_candidate(input(&accepted, &dispatch, outcome, 1)).unwrap();
    assert_eq!(candidate.suffix_records.len(), 2);
    let replay = mixed_replay(accepted, dispatch, candidate);
    let recovered = replay.requests().first().unwrap();
    assert_eq!(
        recovered.known_broker_order_id(),
        Some(&BrokerOrderId::new("BROKER-ORDER-1"))
    );
    assert_eq!(recovered.observed_broker_trade_ids().len(), 1);
}

#[test]
fn multiple_material_trade_broker_ids_without_selected_id_fail_closed() {
    let (identity, accepted) = place_identity();
    let mut other = trade();
    other.broker_trade_id = broker_core::BrokerTradeId::new("TRADE-2");
    other.broker_order_id = Some(BrokerOrderId::new("BROKER-ORDER-2"));
    let outcome = exact_outcome(
        &identity,
        &accepted,
        Stage8a4ExactLifecycle::TerminalFilled,
        order(OrderStatus::Filled, None),
        vec![trade(), other],
    );
    let error = match build_private_durable_candidate(input(&accepted, &accepted, outcome, 1)) {
        Ok(_) => panic!("ambiguous material trade broker ids must fail closed"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        Stage8a4I2CompositionError::MaterialTradeBrokerOrderConflict
    ));
}

fn assert_account_safety_matches_canonical(truth: &BrokerTruthSnapshot, expected_orphans: usize) {
    let canonical = truth.summarize_for_instrument(&instrument());
    let actual = super::super::account_safety_summary(truth, &instrument());
    assert_eq!(
        actual.target_open_positions_count,
        canonical.target_open_positions_count
    );
    assert_eq!(
        actual.account_open_positions_count,
        canonical.account_open_positions_count
    );
    assert_eq!(
        actual.target_active_orders_count,
        canonical.target_active_orders_count
    );
    assert_eq!(
        actual.target_unknown_orders_count,
        canonical.target_unknown_orders_count
    );
    assert_eq!(
        actual.target_terminal_orders_count,
        canonical.target_terminal_orders_count
    );
    assert_eq!(
        actual.target_inconsistent_orders_count,
        canonical.target_inconsistent_orders_count
    );
    assert_eq!(
        actual.account_active_orders_count,
        canonical.account_active_orders_count
    );
    assert_eq!(
        actual.account_unknown_orders_count,
        canonical.account_unknown_orders_count
    );
    assert_eq!(
        actual.account_orphan_orders_count,
        canonical.account_orphan_orders_count
    );
    assert_eq!(
        actual.other_symbol_active_orders_count,
        canonical.other_symbol_active_orders_count
    );
    assert_eq!(actual.account_orphan_orders_count, expected_orphans);
}

#[test]
fn account_safety_uses_canonical_broker_truth_for_all_orphan_classes() {
    let clean = broker_truth(
        vec![order(
            OrderStatus::Working,
            Some(BrokerOrderId::new("BROKER-ORDER-1")),
        )],
        vec![],
        vec![instrument_spec()],
    );
    assert_account_safety_matches_canonical(&clean, 0);

    let mut missing_correlation = order(OrderStatus::Working, None);
    missing_correlation.client_order_id = None;
    assert_account_safety_matches_canonical(
        &broker_truth(vec![missing_correlation], vec![], vec![instrument_spec()]),
        1,
    );

    assert_account_safety_matches_canonical(
        &broker_truth(
            vec![order(
                OrderStatus::Working,
                Some(BrokerOrderId::new("BROKER-ORDER-1")),
            )],
            vec![],
            vec![],
        ),
        1,
    );

    let filled = order(
        OrderStatus::Filled,
        Some(BrokerOrderId::new("BROKER-ORDER-1")),
    );
    assert_account_safety_matches_canonical(
        &broker_truth(vec![filled.clone()], vec![], vec![instrument_spec()]),
        1,
    );

    let mut account_mismatch = trade();
    account_mismatch.account_id = BrokerAccountId::new("ACC_TEST_OTHER");
    assert_account_safety_matches_canonical(
        &broker_truth(
            vec![filled.clone()],
            vec![account_mismatch],
            vec![instrument_spec()],
        ),
        1,
    );

    let mut instrument_mismatch = trade();
    instrument_mismatch.instrument.symbol = "OTHER".into();
    instrument_mismatch.instrument.venue_symbol = Some("OTHER@RTSX".into());
    assert_account_safety_matches_canonical(
        &broker_truth(
            vec![filled.clone()],
            vec![instrument_mismatch],
            vec![instrument_spec()],
        ),
        1,
    );

    let mut side_mismatch = trade();
    side_mismatch.side = OrderSide::Sell;
    assert_account_safety_matches_canonical(
        &broker_truth(
            vec![filled.clone()],
            vec![side_mismatch],
            vec![instrument_spec()],
        ),
        1,
    );

    let mut quantity_mismatch = trade();
    quantity_mismatch.qty = Decimal::ONE;
    assert_account_safety_matches_canonical(
        &broker_truth(
            vec![filled],
            vec![quantity_mismatch],
            vec![instrument_spec()],
        ),
        1,
    );
}

#[test]
fn i3_writer_entry_truth_is_exact_request_account_instrument_and_time_bound() {
    let (identity, accepted) = place_identity();
    let clean_truth = broker_truth(vec![], vec![], vec![instrument_spec()]);
    let binding = digest(FP);
    let clean = fresh_writer_admission(&identity, &accepted, clean_truth.clone());
    assert!(
        super::durable_writer_i3::test_writer_entry_safety(clean, &identity, &binding, now(),)
            .is_some()
    );

    let mut another_request = fresh_writer_admission(&identity, &accepted, clean_truth.clone());
    another_request.admitted_request_id = request(999);
    assert!(super::durable_writer_i3::test_writer_entry_safety(
        another_request,
        &identity,
        &binding,
        now(),
    )
    .is_none());

    let mut another_account = fresh_writer_admission(&identity, &accepted, clean_truth.clone());
    another_account.admitted_account_id = BrokerAccountId::new("ACC_TEST_OTHER");
    another_account.truth.account_id = BrokerAccountId::new("ACC_TEST_OTHER");
    assert!(super::durable_writer_i3::test_writer_entry_safety(
        another_account,
        &identity,
        &binding,
        now(),
    )
    .is_none());

    let mut stale = fresh_writer_admission(&identity, &accepted, clean_truth);
    stale.writer_entry_valid_until = now();
    assert!(
        super::durable_writer_i3::test_writer_entry_safety(stale, &identity, &binding, now(),)
            .is_none()
    );
}

#[test]
fn i3_writer_entry_detects_changed_current_orphan_state() {
    let (identity, accepted) = place_identity();
    let binding = digest(FP);
    let clean = fresh_writer_admission(
        &identity,
        &accepted,
        broker_truth(vec![], vec![], vec![instrument_spec()]),
    );
    let clean_safety =
        super::durable_writer_i3::test_writer_entry_safety(clean, &identity, &binding, now())
            .unwrap();

    let mut orphan = order(OrderStatus::Working, None);
    orphan.client_order_id = None;
    let changed = fresh_writer_admission(
        &identity,
        &accepted,
        broker_truth(vec![orphan], vec![], vec![instrument_spec()]),
    );
    let changed_safety =
        super::durable_writer_i3::test_writer_entry_safety(changed, &identity, &binding, now())
            .unwrap();
    assert_ne!(clean_safety, changed_safety);
}

#[test]
fn cancel_target_cross_binding_is_mandatory() {
    let (identity, accepted) = cancel_identity();
    let mut outcome = exact_outcome(
        &identity,
        &accepted,
        Stage8a4ExactLifecycle::TerminalCancelled,
        order(
            OrderStatus::Canceled,
            Some(BrokerOrderId::new("BROKER-ORDER-1")),
        ),
        vec![],
    );
    outcome.context.known_broker_order_id = Some(BrokerOrderId::new("OTHER"));
    let error = match build_private_durable_candidate(input(&accepted, &accepted, outcome, 1)) {
        Ok(_) => panic!("cancel target mismatch must fail closed"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        Stage8a4I2CompositionError::CommandBindingMismatch
    ));
}

fn place_working_outcome(
    identity: &Stage6DurableRequestIdentityV1,
    accepted: &Stage6JournalRecordV1,
) -> Stage8a4AuthoritativeReconciliationOutcome {
    exact_outcome(
        identity,
        accepted,
        Stage8a4ExactLifecycle::Working,
        order(
            OrderStatus::Working,
            Some(BrokerOrderId::new("BROKER-ORDER-1")),
        ),
        vec![],
    )
}

fn assert_command_cross_binding_fails(
    authority: &Stage6JournalRecordV1,
    cursor: &Stage6JournalRecordV1,
    outcome: Stage8a4AuthoritativeReconciliationOutcome,
) {
    assert!(build_private_durable_candidate(input(authority, cursor, outcome, 1)).is_err());
}

#[test]
fn place_outcome_with_cancel_command_authority_fails_before_candidate() {
    let (place_identity, place_accepted) = place_identity();
    let mut outcome = place_working_outcome(&place_identity, &place_accepted);
    let (_, cancel_accepted) = accepted_cancel(
        &cancel_command(place_identity.strategy_request_id()),
        attribution("CANCEL"),
    );
    rebind_outcome_to_accepted(&mut outcome, &cancel_accepted);
    assert_command_cross_binding_fails(&cancel_accepted, &place_accepted, outcome);
}

#[test]
fn cancel_outcome_with_place_command_authority_fails_before_candidate() {
    let (cancel_identity, cancel_accepted) = cancel_identity();
    let mut outcome = exact_outcome(
        &cancel_identity,
        &cancel_accepted,
        Stage8a4ExactLifecycle::Working,
        order(
            OrderStatus::Working,
            Some(BrokerOrderId::new("BROKER-ORDER-1")),
        ),
        vec![],
    );
    let place = place_command(cancel_identity.strategy_request_id());
    let (_, place_accepted) = accepted_place(&place, attribution("ENTRY"));
    rebind_outcome_to_accepted(&mut outcome, &place_accepted);
    assert_command_cross_binding_fails(&place_accepted, &cancel_accepted, outcome);
}

#[test]
fn place_side_drift_fails_before_candidate() {
    let (identity, accepted) = place_identity();
    let mut outcome = place_working_outcome(&identity, &accepted);
    let mut drifted = place_command(identity.strategy_request_id());
    drifted.side = OrderSide::Sell;
    let (_, drifted_accepted) = accepted_place(&drifted, attribution("ENTRY"));
    rebind_outcome_to_accepted(&mut outcome, &drifted_accepted);
    assert_command_cross_binding_fails(&drifted_accepted, &accepted, outcome);
}

#[test]
fn place_quantity_drift_fails_before_candidate() {
    let (identity, accepted) = place_identity();
    let mut outcome = place_working_outcome(&identity, &accepted);
    let mut drifted = place_command(identity.strategy_request_id());
    drifted.qty = Decimal::from(3);
    let (_, drifted_accepted) = accepted_place(&drifted, attribution("ENTRY"));
    rebind_outcome_to_accepted(&mut outcome, &drifted_accepted);
    assert_command_cross_binding_fails(&drifted_accepted, &accepted, outcome);
}

#[test]
fn place_order_type_drift_fails_before_candidate() {
    let (identity, accepted) = place_identity();
    let mut outcome = place_working_outcome(&identity, &accepted);
    let mut drifted = place_command(identity.strategy_request_id());
    drifted.order_type = OrderType::Market;
    drifted.limit_price = None;
    let (_, drifted_accepted) = accepted_place(&drifted, attribution("ENTRY"));
    rebind_outcome_to_accepted(&mut outcome, &drifted_accepted);
    assert_command_cross_binding_fails(&drifted_accepted, &accepted, outcome);
}

#[test]
fn place_time_in_force_drift_fails_before_candidate() {
    let (identity, accepted) = place_identity();
    let mut outcome = place_working_outcome(&identity, &accepted);
    let mut drifted = place_command(identity.strategy_request_id());
    drifted.time_in_force = TimeInForce::GoodTillCancel;
    let (_, drifted_accepted) = accepted_place(&drifted, attribution("ENTRY"));
    rebind_outcome_to_accepted(&mut outcome, &drifted_accepted);
    assert_command_cross_binding_fails(&drifted_accepted, &accepted, outcome);
}

#[test]
fn place_limit_price_drift_fails_before_candidate() {
    let (identity, accepted) = place_identity();
    let mut outcome = place_working_outcome(&identity, &accepted);
    let mut drifted = place_command(identity.strategy_request_id());
    drifted.limit_price = Some(Decimal::from(2209));
    let (_, drifted_accepted) = accepted_place(&drifted, attribution("ENTRY"));
    rebind_outcome_to_accepted(&mut outcome, &drifted_accepted);
    assert_command_cross_binding_fails(&drifted_accepted, &accepted, outcome);
}

#[test]
fn market_vs_limit_price_shape_drift_fails_before_candidate() {
    let mut market = place_command(request(1));
    market.order_type = OrderType::Market;
    market.limit_price = None;
    let (market_identity, market_accepted) = accepted_place(&market, attribution("ENTRY"));
    let mut outcome = place_working_outcome(&market_identity, &market_accepted);
    outcome.context.order_type = OrderType::Market;
    outcome.context.limit_price = None;
    rebind_outcome_to_accepted(&mut outcome, &market_accepted);

    let (_, limit_accepted) = accepted_place(&place_command(request(1)), attribution("ENTRY"));
    rebind_outcome_to_accepted(&mut outcome, &limit_accepted);
    assert_command_cross_binding_fails(&limit_accepted, &market_accepted, outcome);
}

#[test]
fn attribution_drift_fails_before_candidate() {
    let (identity, accepted) = place_identity();
    let mut outcome = place_working_outcome(&identity, &accepted);
    let mut drifted = place_command(identity.strategy_request_id());
    let drifted_attribution = attribution_for("cycle0002", "ENTRY");
    drifted.comment = Some(drifted_attribution.internal_comment().to_string());
    let (_, drifted_accepted) = accepted_place(&drifted, drifted_attribution);
    rebind_outcome_to_accepted(&mut outcome, &drifted_accepted);
    assert_command_cross_binding_fails(&drifted_accepted, &accepted, outcome);
}

#[test]
fn cancel_target_broker_order_drift_fails_before_candidate() {
    let (identity, accepted) = cancel_identity();
    let mut outcome = exact_outcome(
        &identity,
        &accepted,
        Stage8a4ExactLifecycle::Working,
        order(
            OrderStatus::Working,
            Some(BrokerOrderId::new("BROKER-ORDER-1")),
        ),
        vec![],
    );
    let mut drifted = cancel_command(identity.strategy_request_id());
    drifted.order_id = BrokerOrderId::new("BROKER-ORDER-OTHER");
    let (_, drifted_accepted) = accepted_cancel(&drifted, attribution("CANCEL"));
    rebind_outcome_to_accepted(&mut outcome, &drifted_accepted);
    assert_command_cross_binding_fails(&drifted_accepted, &accepted, outcome);
}

#[test]
fn cancel_target_client_order_drift_fails_before_candidate() {
    let (identity, accepted) = cancel_identity();
    let mut outcome = exact_outcome(
        &identity,
        &accepted,
        Stage8a4ExactLifecycle::Working,
        order(
            OrderStatus::Working,
            Some(BrokerOrderId::new("BROKER-ORDER-1")),
        ),
        vec![],
    );
    let mut drifted = cancel_command(identity.strategy_request_id());
    drifted.client_order_id = Some(ClientOrderId::new("CLIENT-OTHER").unwrap());
    let (_, drifted_accepted) = accepted_cancel(&drifted, attribution("CANCEL"));
    rebind_outcome_to_accepted(&mut outcome, &drifted_accepted);
    assert_command_cross_binding_fails(&drifted_accepted, &accepted, outcome);
}

#[test]
fn accepted_command_payload_digest_drift_fails_before_candidate() {
    let (identity, accepted) = place_identity();
    let outcome = place_working_outcome(&identity, &accepted);
    let mut drifted = place_command(identity.strategy_request_id());
    drifted.ttl_ms = Some(9_000);
    let (_, drifted_accepted) = accepted_place(&drifted, attribution("ENTRY"));
    assert_command_cross_binding_fails(&drifted_accepted, &accepted, outcome);
}

#[test]
fn exact_accepted_command_produces_deterministic_candidate_and_key() {
    let build = || {
        let (identity, accepted) = place_identity();
        let outcome = place_working_outcome(&identity, &accepted);
        build_private_durable_candidate(input(&accepted, &accepted, outcome, 1)).unwrap()
    };
    let first = build();
    let second = build();
    assert_eq!(
        first.transition_record.encode_canonical(),
        second.transition_record.encode_canonical()
    );
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
}

#[test]
fn candidate_is_pure_and_deterministic_for_identical_inputs() {
    let build = || {
        let (identity, accepted) = place_identity();
        let outcome = exact_outcome(
            &identity,
            &accepted,
            Stage8a4ExactLifecycle::TerminalFilled,
            order(
                OrderStatus::Filled,
                Some(BrokerOrderId::new("BROKER-ORDER-1")),
            ),
            vec![trade()],
        );
        build_private_durable_candidate(input(&accepted, &accepted, outcome, 3)).unwrap()
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
