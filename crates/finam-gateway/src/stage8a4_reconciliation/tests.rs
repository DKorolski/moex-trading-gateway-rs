use super::*;
use broker_core::{
    BrokerInstrumentSpec, BrokerKind, BrokerSymbol, Exchange, InstrumentMapEntry, InternalSymbol,
    Market,
};
use uuid::Uuid;

const FP: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const FP_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

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

fn context(
    now: DateTime<Utc>,
    order_type: OrderType,
    limit_price: Option<Decimal>,
    known_broker_order_id: Option<BrokerOrderId>,
) -> Stage8a4DurableRequestContext {
    Stage8a4DurableRequestContext {
        request_id: StrategyRequestId::new(Uuid::from_u128(1)),
        client_order_id: ClientOrderId::new("CLIENT-0001").unwrap(),
        account_id: BrokerAccountId::new("ACC_TEST_0001"),
        instrument: instrument(),
        side: OrderSide::Buy,
        qty: Decimal::from(6),
        order_type,
        time_in_force: TimeInForce::Day,
        limit_price,
        known_broker_order_id,
        possible_effect_at: now - Duration::seconds(30),
        event_start: now - Duration::seconds(25),
        event_end: now - Duration::seconds(5),
        durable_binding_sha256: FP.into(),
    }
}

fn policy(now: DateTime<Utc>) -> Stage8a4ReconciliationPolicy {
    Stage8a4ReconciliationPolicy {
        trusted_now: now,
        max_source_age: Duration::minutes(2),
        max_cross_source_skew: Duration::seconds(30),
        max_trade_intervals: 8,
        max_interval_split_depth: 4,
        policy_binding_sha256: FP.into(),
    }
}

fn timing(now: DateTime<Utc>) -> Stage8a4SourceTiming {
    Stage8a4SourceTiming {
        request_started_at: now - Duration::seconds(4),
        response_received_at: now - Duration::seconds(3),
    }
}

fn evidence(
    now: DateTime<Utc>,
    context: &Stage8a4DurableRequestContext,
    snapshot: &BrokerTruthSnapshot,
) -> Stage8a4SourceEvidence {
    let intervals = vec![Stage8a4TradeIntervalProof {
        start_inclusive: context.event_start,
        end_exclusive: context.event_end,
        requested_limit: 100,
        returned_count: snapshot.trades.len(),
        request_started_at: now - Duration::seconds(4),
        response_received_at: now - Duration::seconds(3),
        split_depth: 0,
    }];
    let refs = intervals.iter().collect::<Vec<_>>();
    Stage8a4SourceEvidence {
        orders: Stage8a4NonPaginatedOrdersSnapshotComplete {
            timing: timing(now),
        },
        trades: Stage8a4BoundedTradeHistoryComplete {
            interval_coverage_sha256: interval_coverage_fingerprint(&refs),
            intervals,
        },
        positions: Stage8a4CompletePositionsSnapshot {
            timing: timing(now),
        },
        instruments: Stage8a4InstrumentCompletenessEvidence::ExactTargetResolved {
            timing: timing(now),
        },
        exact_order_observation: None,
        canonical_truth_payload_sha256: canonical_truth_binding(snapshot),
        acquisition_policy_sha256: FP.into(),
    }
}

fn order(
    now: DateTime<Utc>,
    status: OrderStatus,
    filled_qty: Decimal,
    order_type: OrderType,
    limit_price: Option<Decimal>,
) -> BrokerOrderSnapshot {
    BrokerOrderSnapshot {
        account_id: BrokerAccountId::new("ACC_TEST_0001"),
        broker_order_id: Some(BrokerOrderId::new("BROKER-ORDER-1")),
        client_order_id: Some(ClientOrderId::new("CLIENT-0001").unwrap()),
        instrument: instrument(),
        side: OrderSide::Buy,
        order_type,
        time_in_force: Some(TimeInForce::Day),
        lifecycle: BrokerOrderSnapshot::lifecycle_for(&status),
        status,
        qty: Decimal::from(6),
        filled_qty,
        remaining_qty: Some(Decimal::from(6) - filled_qty),
        limit_price,
        broker_asset_id: Some("ASSET_IMOEXF".into()),
        board: Some("RFUD".into()),
        expiration_date: None,
        source_ts: Some(now - Duration::seconds(10)),
        received_ts: now - Duration::seconds(3),
    }
}

fn trade(now: DateTime<Utc>, id: &str, qty: Decimal, received_offset: i64) -> BrokerTradeSnapshot {
    BrokerTradeSnapshot {
        account_id: BrokerAccountId::new("ACC_TEST_0001"),
        broker_trade_id: broker_core::BrokerTradeId::new(id),
        broker_order_id: Some(BrokerOrderId::new("BROKER-ORDER-1")),
        client_order_id: Some(ClientOrderId::new("CLIENT-0001").unwrap()),
        instrument: instrument(),
        side: OrderSide::Buy,
        qty,
        price: Decimal::from(2210),
        gross_amount: None,
        commission: None,
        broker_asset_id: Some("ASSET_IMOEXF".into()),
        board: Some("RFUD".into()),
        expiration_date: None,
        source_ts: now - Duration::seconds(10),
        received_ts: now - Duration::seconds(received_offset),
    }
}

fn truth(
    now: DateTime<Utc>,
    orders: Vec<BrokerOrderSnapshot>,
    trades: Vec<BrokerTradeSnapshot>,
) -> BrokerTruthSnapshot {
    BrokerTruthSnapshot {
        account_id: BrokerAccountId::new("ACC_TEST_0001"),
        orders,
        positions: vec![],
        cash: None,
        trades,
        instruments: vec![instrument_spec()],
        received_ts: now - Duration::seconds(3),
    }
}

fn reconcile(
    now: DateTime<Utc>,
    context: Stage8a4DurableRequestContext,
    truth: BrokerTruthSnapshot,
) -> Stage8a4ReconciliationDiagnostic {
    let evidence = evidence(now, &context, &truth);
    let policy = policy(now);
    let admitted = admit_stage8a4_broker_truth(&context, &policy, truth, evidence).unwrap();
    reduce_stage8a4_reconciliation(context, admitted, policy)
}

fn admission_error(
    result: Result<Stage8a4FreshTruthAdmission, Box<Stage8a4ReconciliationDiagnostic>>,
) -> Stage8a4ReconciliationDiagnostic {
    match result {
        Ok(_) => panic!("expected Stage 8A-4 admission failure"),
        Err(error) => *error,
    }
}

#[test]
fn cancelled_and_expired_partial_fills_preserve_both_dimensions() {
    let now = Utc::now();
    for (status, lifecycle) in [
        (
            OrderStatus::Canceled,
            Stage8a4ExactLifecycle::TerminalCancelled,
        ),
        (
            OrderStatus::Expired,
            Stage8a4ExactLifecycle::TerminalExpired,
        ),
    ] {
        let result = reconcile(
            now,
            context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
            truth(
                now,
                vec![order(
                    now,
                    status,
                    Decimal::from(2),
                    OrderType::Limit,
                    Some(Decimal::from(2210)),
                )],
                vec![trade(now, "TRADE-1", Decimal::from(2), 3)],
            ),
        );
        assert_eq!(result.outcome, Stage8a4OutcomeKind::ExactOrderState);
        assert_eq!(result.lifecycle, Some(lifecycle));
        assert_eq!(
            result.fill,
            Some(Stage8a4FillEffect::Partial {
                filled_qty: Decimal::from(2)
            })
        );
        assert!(!result.retry_authorized && !result.send_authorized);
    }
}

#[test]
fn status_quantity_and_trade_quantity_contradictions_conflict() {
    let now = Utc::now();
    let filled_partial = reconcile(
        now,
        context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
        truth(
            now,
            vec![order(
                now,
                OrderStatus::Filled,
                Decimal::from(2),
                OrderType::Limit,
                Some(Decimal::from(2210)),
            )],
            vec![trade(now, "TRADE-1", Decimal::from(2), 3)],
        ),
    );
    assert_eq!(filled_partial.outcome, Stage8a4OutcomeKind::Conflict);
    assert_eq!(
        filled_partial.reason,
        Stage8a4ReconciliationReason::OrderQuantityContradiction
    );

    let trade_mismatch = reconcile(
        now,
        context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
        truth(
            now,
            vec![order(
                now,
                OrderStatus::PartiallyFilled,
                Decimal::from(2),
                OrderType::Limit,
                Some(Decimal::from(2210)),
            )],
            vec![trade(now, "TRADE-1", Decimal::ONE, 3)],
        ),
    );
    assert_eq!(
        trade_mismatch.reason,
        Stage8a4ReconciliationReason::TradeQuantityContradiction
    );
}

#[test]
fn identical_trade_duplicates_count_once_and_conflicting_duplicates_fail() {
    let now = Utc::now();
    let first = trade(now, "TRADE-1", Decimal::from(2), 3);
    let mut duplicate = first.clone();
    duplicate.received_ts = now - Duration::seconds(2);
    let exact = reconcile(
        now,
        context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
        truth(
            now,
            vec![order(
                now,
                OrderStatus::PartiallyFilled,
                Decimal::from(2),
                OrderType::Limit,
                Some(Decimal::from(2210)),
            )],
            vec![first.clone(), duplicate],
        ),
    );
    assert_eq!(exact.outcome, Stage8a4OutcomeKind::ExactOrderState);
    assert_eq!(exact.matching_trade_count, 1);

    let mut conflicting = first.clone();
    conflicting.price = Decimal::from(2211);
    let conflict = reconcile(
        now,
        context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
        truth(
            now,
            vec![order(
                now,
                OrderStatus::PartiallyFilled,
                Decimal::from(2),
                OrderType::Limit,
                Some(Decimal::from(2210)),
            )],
            vec![first, conflicting],
        ),
    );
    assert_eq!(
        conflict.reason,
        Stage8a4ReconciliationReason::TradeIdentityConflict
    );
}

#[test]
fn saturated_or_gapped_trade_history_is_not_admitted_even_with_identical_timestamps() {
    let now = Utc::now();
    let context = context(now, OrderType::Limit, Some(Decimal::from(2210)), None);
    let snapshot = truth(
        now,
        vec![order(
            now,
            OrderStatus::PartiallyFilled,
            Decimal::from(2),
            OrderType::Limit,
            Some(Decimal::from(2210)),
        )],
        vec![
            trade(now, "TRADE-1", Decimal::ONE, 3),
            trade(now, "TRADE-2", Decimal::ONE, 3),
        ],
    );
    let mut saturated = evidence(now, &context, &snapshot);
    saturated.trades.intervals[0].requested_limit = 2;
    let refs = saturated.trades.intervals.iter().collect::<Vec<_>>();
    saturated.trades.interval_coverage_sha256 = interval_coverage_fingerprint(&refs);
    let rejected = admission_error(admit_stage8a4_broker_truth(
        &context,
        &policy(now),
        snapshot,
        saturated,
    ));
    assert_eq!(rejected.outcome, Stage8a4OutcomeKind::StillUnknown);

    let empty_snapshot = truth(now, vec![], vec![]);
    let mut gapped = evidence(now, &context, &empty_snapshot);
    gapped.trades.intervals[0].start_inclusive = context.event_start + Duration::seconds(1);
    let refs = gapped.trades.intervals.iter().collect::<Vec<_>>();
    gapped.trades.interval_coverage_sha256 = interval_coverage_fingerprint(&refs);
    let rejected = admission_error(admit_stage8a4_broker_truth(
        &context,
        &policy(now),
        empty_snapshot,
        gapped,
    ));
    assert_eq!(
        rejected.reason,
        Stage8a4ReconciliationReason::SourceIncomplete
    );
}

#[test]
fn exact_lookup_not_found_never_becomes_no_match_and_disagreement_conflicts() {
    let now = Utc::now();
    let first_context = context(
        now,
        OrderType::Limit,
        Some(Decimal::from(2210)),
        Some(BrokerOrderId::new("BROKER-ORDER-1")),
    );
    let empty_snapshot = truth(now, vec![], vec![]);
    let not_found = evidence(now, &first_context, &empty_snapshot);
    let admitted =
        admit_stage8a4_broker_truth(&first_context, &policy(now), empty_snapshot, not_found)
            .unwrap();
    let result = reduce_stage8a4_reconciliation(first_context, admitted, policy(now));
    assert_eq!(result.outcome, Stage8a4OutcomeKind::StillUnknown);

    let context = context(
        now,
        OrderType::Limit,
        Some(Decimal::from(2210)),
        Some(BrokerOrderId::new("BROKER-ORDER-1")),
    );
    let empty_snapshot = truth(now, vec![], vec![]);
    let mut disagreement = evidence(now, &context, &empty_snapshot);
    let mut exact = order(
        now,
        OrderStatus::Working,
        Decimal::ZERO,
        OrderType::Limit,
        Some(Decimal::from(2210)),
    );
    exact.broker_order_id = Some(BrokerOrderId::new("OTHER"));
    disagreement.exact_order_observation = Some(Stage8a4ExactOrderObservation {
        order: exact,
        timing: timing(now),
    });
    let rejected = admission_error(admit_stage8a4_broker_truth(
        &context,
        &policy(now),
        empty_snapshot,
        disagreement,
    ));
    assert_eq!(rejected.outcome, Stage8a4OutcomeKind::Conflict);
}

#[test]
fn tier3_never_weakens_order_type_tif_or_limit_price() {
    let now = Utc::now();
    let mut no_exact_id = order(
        now,
        OrderStatus::Working,
        Decimal::ZERO,
        OrderType::Market,
        None,
    );
    no_exact_id.client_order_id = None;
    no_exact_id.broker_order_id = None;
    let result = reconcile(
        now,
        context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
        truth(now, vec![no_exact_id], vec![]),
    );
    assert_eq!(result.outcome, Stage8a4OutcomeKind::StillUnknown);

    let mut missing_tif = order(
        now,
        OrderStatus::Working,
        Decimal::ZERO,
        OrderType::Limit,
        Some(Decimal::from(2210)),
    );
    missing_tif.client_order_id = None;
    missing_tif.broker_order_id = None;
    missing_tif.time_in_force = None;
    let result = reconcile(
        now,
        context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
        truth(now, vec![missing_tif], vec![]),
    );
    assert_eq!(
        result.reason,
        Stage8a4ReconciliationReason::MissingRequiredShape
    );
}

#[test]
fn exact_identity_precedence_detects_disagreement() {
    let now = Utc::now();
    let context = context(
        now,
        OrderType::Limit,
        Some(Decimal::from(2210)),
        Some(BrokerOrderId::new("BROKER-ORDER-2")),
    );
    let first = order(
        now,
        OrderStatus::Working,
        Decimal::ZERO,
        OrderType::Limit,
        Some(Decimal::from(2210)),
    );
    let mut second = first.clone();
    second.client_order_id = Some(ClientOrderId::new("OTHER-CLIENT").unwrap());
    second.broker_order_id = Some(BrokerOrderId::new("BROKER-ORDER-2"));
    let result = reconcile(now, context, truth(now, vec![first, second], vec![]));
    assert_eq!(
        result.reason,
        Stage8a4ReconciliationReason::ExactIdentityDisagreement
    );
}

#[test]
fn shuffled_orders_and_duplicate_ordering_are_byte_stable() {
    let now = Utc::now();
    let selected = order(
        now,
        OrderStatus::PartiallyFilled,
        Decimal::from(2),
        OrderType::Limit,
        Some(Decimal::from(2210)),
    );
    let mut unrelated = selected.clone();
    unrelated.client_order_id = Some(ClientOrderId::new("OTHER-CLIENT").unwrap());
    unrelated.broker_order_id = Some(BrokerOrderId::new("OTHER-ORDER"));
    unrelated.side = OrderSide::Sell;
    let trade_a = trade(now, "TRADE-1", Decimal::ONE, 3);
    let trade_b = trade(now, "TRADE-2", Decimal::ONE, 3);

    let left = reconcile(
        now,
        context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
        truth(
            now,
            vec![selected.clone(), unrelated.clone()],
            vec![trade_a.clone(), trade_b.clone()],
        ),
    );
    let right = reconcile(
        now,
        context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
        truth(now, vec![unrelated, selected], vec![trade_b, trade_a]),
    );
    assert_eq!(
        serde_json::to_vec(&left).unwrap(),
        serde_json::to_vec(&right).unwrap()
    );
}

#[test]
fn bounded_split_policy_rejects_unsplittable_interval() {
    let now = Utc::now();
    let policy = policy(now);
    let splittable = Stage8a4TradeIntervalProof {
        start_inclusive: now,
        end_exclusive: now + Duration::microseconds(10),
        requested_limit: 100,
        returned_count: 100,
        request_started_at: now,
        response_received_at: now,
        split_depth: 0,
    };
    assert_eq!(
        deterministic_interval_split(&splittable, 0, &policy),
        Some((
            (now, now + Duration::microseconds(5)),
            (
                now + Duration::microseconds(5),
                now + Duration::microseconds(10)
            )
        ))
    );
    assert!(
        deterministic_interval_split(&splittable, policy.max_interval_split_depth, &policy)
            .is_none()
    );
    let identical = Stage8a4TradeIntervalProof {
        start_inclusive: now,
        end_exclusive: now,
        requested_limit: 100,
        returned_count: 100,
        request_started_at: now,
        response_received_at: now,
        split_depth: 0,
    };
    assert!(deterministic_interval_split(&identical, 0, &policy).is_none());
}

#[test]
fn exact_lookup_unavailable_is_also_non_terminal() {
    let now = Utc::now();
    let context = context(
        now,
        OrderType::Market,
        None,
        Some(BrokerOrderId::new("BROKER-ORDER-1")),
    );
    let empty_snapshot = truth(now, vec![], vec![]);
    let evidence = evidence(now, &context, &empty_snapshot);
    let admitted =
        admit_stage8a4_broker_truth(&context, &policy(now), empty_snapshot, evidence).unwrap();
    let result = reduce_stage8a4_reconciliation(context, admitted, policy(now));
    assert_eq!(result.outcome, Stage8a4OutcomeKind::StillUnknown);
    assert!(!result.retry_authorized && !result.send_authorized);
}

#[test]
fn exact_lifecycle_fill_matrix_covers_zero_partial_and_full() {
    let now = Utc::now();
    let cases = [
        (
            OrderStatus::Working,
            Decimal::ZERO,
            Stage8a4ExactLifecycle::Working,
            Stage8a4FillEffect::Zero,
        ),
        (
            OrderStatus::PartiallyFilled,
            Decimal::from(2),
            Stage8a4ExactLifecycle::Working,
            Stage8a4FillEffect::Partial {
                filled_qty: Decimal::from(2),
            },
        ),
        (
            OrderStatus::Filled,
            Decimal::from(6),
            Stage8a4ExactLifecycle::TerminalFilled,
            Stage8a4FillEffect::Full {
                filled_qty: Decimal::from(6),
            },
        ),
        (
            OrderStatus::Canceled,
            Decimal::ZERO,
            Stage8a4ExactLifecycle::TerminalCancelled,
            Stage8a4FillEffect::Zero,
        ),
        (
            OrderStatus::Expired,
            Decimal::ZERO,
            Stage8a4ExactLifecycle::TerminalExpired,
            Stage8a4FillEffect::Zero,
        ),
        (
            OrderStatus::Rejected,
            Decimal::ZERO,
            Stage8a4ExactLifecycle::TerminalRejected,
            Stage8a4FillEffect::Zero,
        ),
    ];
    for (status, filled, lifecycle, fill) in cases {
        let trades = if filled == Decimal::ZERO {
            vec![]
        } else {
            vec![trade(now, "TRADE-1", filled, 3)]
        };
        let result = reconcile(
            now,
            context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
            truth(
                now,
                vec![order(
                    now,
                    status,
                    filled,
                    OrderType::Limit,
                    Some(Decimal::from(2210)),
                )],
                trades,
            ),
        );
        assert_eq!(result.outcome, Stage8a4OutcomeKind::ExactOrderState);
        assert_eq!(result.lifecycle, Some(lifecycle));
        assert_eq!(result.fill, Some(fill));
    }
}

#[test]
fn partial_rejected_and_remaining_inconsistencies_conflict() {
    let now = Utc::now();
    for (status, filled) in [
        (OrderStatus::PartiallyFilled, Decimal::ZERO),
        (OrderStatus::PartiallyFilled, Decimal::from(6)),
        (OrderStatus::Rejected, Decimal::ONE),
    ] {
        let trades = if filled == Decimal::ZERO {
            vec![]
        } else {
            vec![trade(now, "TRADE-1", filled, 3)]
        };
        let result = reconcile(
            now,
            context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
            truth(
                now,
                vec![order(
                    now,
                    status,
                    filled,
                    OrderType::Limit,
                    Some(Decimal::from(2210)),
                )],
                trades,
            ),
        );
        assert_eq!(result.outcome, Stage8a4OutcomeKind::Conflict);
        assert_eq!(
            result.reason,
            Stage8a4ReconciliationReason::OrderQuantityContradiction
        );
    }

    let mut inconsistent = order(
        now,
        OrderStatus::PartiallyFilled,
        Decimal::from(2),
        OrderType::Limit,
        Some(Decimal::from(2210)),
    );
    inconsistent.remaining_qty = Some(Decimal::from(5));
    let result = reconcile(
        now,
        context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
        truth(
            now,
            vec![inconsistent],
            vec![trade(now, "TRADE-1", Decimal::from(2), 3)],
        ),
    );
    assert_eq!(
        result.reason,
        Stage8a4ReconciliationReason::OrderQuantityContradiction
    );
}

#[test]
fn tier3_exact_shape_selects_once_and_rejects_price_or_tif_drift() {
    let now = Utc::now();
    let mut exact_shape = order(
        now,
        OrderStatus::Working,
        Decimal::ZERO,
        OrderType::Limit,
        Some(Decimal::from(2210)),
    );
    exact_shape.client_order_id = None;
    exact_shape.broker_order_id = None;
    let selected = reconcile(
        now,
        context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
        truth(now, vec![exact_shape.clone()], vec![]),
    );
    assert_eq!(
        selected.reason,
        Stage8a4ReconciliationReason::ExactTier3BoundShape
    );

    let mut wrong_price = exact_shape.clone();
    wrong_price.limit_price = Some(Decimal::from(2211));
    let no_match = reconcile(
        now,
        context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
        truth(now, vec![wrong_price], vec![]),
    );
    assert_eq!(no_match.outcome, Stage8a4OutcomeKind::StillUnknown);

    let mut wrong_tif = exact_shape;
    wrong_tif.time_in_force = Some(TimeInForce::GoodTillCancel);
    let no_match = reconcile(
        now,
        context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
        truth(now, vec![wrong_tif], vec![]),
    );
    assert_eq!(no_match.outcome, Stage8a4OutcomeKind::StillUnknown);
}

#[test]
fn exact_lookup_does_not_replace_account_wide_safety_snapshot() {
    let now = Utc::now();
    let context = context(
        now,
        OrderType::Limit,
        Some(Decimal::from(2210)),
        Some(BrokerOrderId::new("BROKER-ORDER-1")),
    );
    let empty_snapshot = truth(now, vec![], vec![]);
    let mut evidence = evidence(now, &context, &empty_snapshot);
    evidence.exact_order_observation = Some(Stage8a4ExactOrderObservation {
        order: order(
            now,
            OrderStatus::Working,
            Decimal::ZERO,
            OrderType::Limit,
            Some(Decimal::from(2210)),
        ),
        timing: timing(now),
    });
    evidence.instruments = Stage8a4InstrumentCompletenessEvidence::FullRegistryCursorExhausted {
        timing: timing(now),
    };
    let admitted =
        admit_stage8a4_broker_truth(&context, &policy(now), empty_snapshot, evidence).unwrap();
    assert_eq!(admitted.account_active_orders_count, 0);
    assert_eq!(admitted.target_active_orders_count, 0);
    let result = reduce_stage8a4_reconciliation(context, admitted, policy(now));
    assert_eq!(result.outcome, Stage8a4OutcomeKind::ExactOrderState);
    assert_eq!(result.account_active_orders_count, 0);
}

#[test]
fn cross_source_skew_fails_admission() {
    let now = Utc::now();
    let context = context(now, OrderType::Market, None, None);
    let empty_snapshot = truth(now, vec![], vec![]);
    let mut evidence = evidence(now, &context, &empty_snapshot);
    evidence.positions.timing.response_received_at = now - Duration::seconds(12);
    evidence.positions.timing.request_started_at = now - Duration::seconds(13);
    let mut strict_policy = policy(now);
    strict_policy.max_cross_source_skew = Duration::seconds(5);
    let result = admission_error(admit_stage8a4_broker_truth(
        &context,
        &strict_policy,
        empty_snapshot,
        evidence,
    ));
    assert_eq!(result.reason, Stage8a4ReconciliationReason::SourceStale);
}

#[test]
fn unknown_status_and_missing_exact_shape_remain_unknown() {
    let now = Utc::now();
    let unknown = reconcile(
        now,
        context(now, OrderType::Market, None, None),
        truth(
            now,
            vec![order(
                now,
                OrderStatus::Unknown("BROKER_NEW_STATE".into()),
                Decimal::ZERO,
                OrderType::Market,
                None,
            )],
            vec![],
        ),
    );
    assert_eq!(unknown.outcome, Stage8a4OutcomeKind::StillUnknown);
    assert_eq!(
        unknown.reason,
        Stage8a4ReconciliationReason::UnknownOrderStatus
    );

    let mut missing_tif = order(
        now,
        OrderStatus::Working,
        Decimal::ZERO,
        OrderType::Market,
        None,
    );
    missing_tif.time_in_force = None;
    let missing = reconcile(
        now,
        context(now, OrderType::Market, None, None),
        truth(now, vec![missing_tif], vec![]),
    );
    assert_eq!(missing.outcome, Stage8a4OutcomeKind::StillUnknown);
    assert_eq!(
        missing.reason,
        Stage8a4ReconciliationReason::MissingRequiredShape
    );
}

#[test]
fn admission_cannot_be_reduced_with_another_durable_context() {
    let now = Utc::now();
    let context_a = context(now, OrderType::Market, None, None);
    let snapshot = truth(now, vec![], vec![]);
    let policy = policy(now);
    let admitted = admit_stage8a4_broker_truth(
        &context_a,
        &policy,
        snapshot.clone(),
        evidence(now, &context_a, &snapshot),
    )
    .unwrap();
    let mut context_b = context(now, OrderType::Market, None, None);
    context_b.durable_binding_sha256 = FP_B.into();
    let result = reduce_stage8a4_reconciliation(context_b, admitted, policy);
    assert_eq!(result.outcome, Stage8a4OutcomeKind::StillUnknown);
    assert_eq!(
        result.reason,
        Stage8a4ReconciliationReason::SourceIncomplete
    );
}

#[test]
fn admission_cannot_be_reduced_with_another_policy() {
    let now = Utc::now();
    let context = context(now, OrderType::Market, None, None);
    let snapshot = truth(now, vec![], vec![]);
    let policy_a = policy(now);
    let admitted = admit_stage8a4_broker_truth(
        &context,
        &policy_a,
        snapshot.clone(),
        evidence(now, &context, &snapshot),
    )
    .unwrap();
    let mut policy_b = policy(now);
    policy_b.policy_binding_sha256 = FP_B.into();
    let result = reduce_stage8a4_reconciliation(context, admitted, policy_b);
    assert_eq!(result.outcome, Stage8a4OutcomeKind::StillUnknown);
    assert_eq!(
        result.reason,
        Stage8a4ReconciliationReason::SourceIncomplete
    );
}

#[test]
fn source_evidence_cannot_be_paired_with_another_canonical_payload() {
    let now = Utc::now();
    let context = context(now, OrderType::Limit, Some(Decimal::from(2210)), None);
    let snapshot_a = truth(now, vec![], vec![]);
    let snapshot_b = truth(
        now,
        vec![order(
            now,
            OrderStatus::Working,
            Decimal::ZERO,
            OrderType::Limit,
            Some(Decimal::from(2210)),
        )],
        vec![],
    );
    let rejected = admission_error(admit_stage8a4_broker_truth(
        &context,
        &policy(now),
        snapshot_b,
        evidence(now, &context, &snapshot_a),
    ));
    assert_eq!(rejected.outcome, Stage8a4OutcomeKind::StillUnknown);
    assert_eq!(
        rejected.reason,
        Stage8a4ReconciliationReason::SourceIncomplete
    );
}

#[test]
fn exact_get_request_started_before_possible_effect_is_rejected() {
    let now = Utc::now();
    let context = context(
        now,
        OrderType::Limit,
        Some(Decimal::from(2210)),
        Some(BrokerOrderId::new("BROKER-ORDER-1")),
    );
    let snapshot = truth(now, vec![], vec![]);
    let mut evidence = evidence(now, &context, &snapshot);
    evidence.exact_order_observation = Some(Stage8a4ExactOrderObservation {
        order: order(
            now,
            OrderStatus::Working,
            Decimal::ZERO,
            OrderType::Limit,
            Some(Decimal::from(2210)),
        ),
        timing: Stage8a4SourceTiming {
            request_started_at: context.possible_effect_at - Duration::milliseconds(1),
            response_received_at: now - Duration::seconds(3),
        },
    });
    let rejected = admission_error(admit_stage8a4_broker_truth(
        &context,
        &policy(now),
        snapshot,
        evidence,
    ));
    assert_eq!(rejected.outcome, Stage8a4OutcomeKind::StillUnknown);
    assert_eq!(
        rejected.reason,
        Stage8a4ReconciliationReason::SourceIncomplete
    );
}

#[test]
fn exact_get_staleness_and_cross_source_skew_are_rejected() {
    let now = Utc::now();
    let mut context = context(
        now,
        OrderType::Limit,
        Some(Decimal::from(2210)),
        Some(BrokerOrderId::new("BROKER-ORDER-1")),
    );
    context.possible_effect_at = now - Duration::minutes(5);
    let snapshot = truth(now, vec![], vec![]);
    let mut stale = evidence(now, &context, &snapshot);
    let mut stale_order = order(
        now,
        OrderStatus::Working,
        Decimal::ZERO,
        OrderType::Limit,
        Some(Decimal::from(2210)),
    );
    stale_order.received_ts = now - Duration::minutes(3);
    stale.exact_order_observation = Some(Stage8a4ExactOrderObservation {
        order: stale_order,
        timing: Stage8a4SourceTiming {
            request_started_at: now - Duration::minutes(3) - Duration::seconds(1),
            response_received_at: now - Duration::minutes(3),
        },
    });
    let rejected = admission_error(admit_stage8a4_broker_truth(
        &context,
        &policy(now),
        snapshot.clone(),
        stale,
    ));
    assert_eq!(rejected.reason, Stage8a4ReconciliationReason::SourceStale);

    context.possible_effect_at = now - Duration::seconds(30);
    let mut skewed = evidence(now, &context, &snapshot);
    let mut skewed_order = order(
        now,
        OrderStatus::Working,
        Decimal::ZERO,
        OrderType::Limit,
        Some(Decimal::from(2210)),
    );
    skewed_order.received_ts = now - Duration::seconds(19);
    skewed.exact_order_observation = Some(Stage8a4ExactOrderObservation {
        order: skewed_order,
        timing: Stage8a4SourceTiming {
            request_started_at: now - Duration::seconds(20),
            response_received_at: now - Duration::seconds(19),
        },
    });
    let mut strict = policy(now);
    strict.max_cross_source_skew = Duration::seconds(5);
    let rejected = admission_error(admit_stage8a4_broker_truth(
        &context, &strict, snapshot, skewed,
    ));
    assert_eq!(rejected.reason, Stage8a4ReconciliationReason::SourceStale);
}

#[test]
fn tier1_client_match_cannot_hide_broker_id_contradiction() {
    let now = Utc::now();
    let context = context(
        now,
        OrderType::Limit,
        Some(Decimal::from(2210)),
        Some(BrokerOrderId::new("BROKER-ORDER-1")),
    );
    let mut selected = order(
        now,
        OrderStatus::Working,
        Decimal::ZERO,
        OrderType::Limit,
        Some(Decimal::from(2210)),
    );
    selected.broker_order_id = Some(BrokerOrderId::new("BROKER-ORDER-2"));
    let result = reconcile(now, context, truth(now, vec![selected], vec![]));
    assert_eq!(result.outcome, Stage8a4OutcomeKind::Conflict);
    assert_eq!(
        result.reason,
        Stage8a4ReconciliationReason::ExactIdentityDisagreement
    );
}

#[test]
fn tier2_broker_match_cannot_hide_client_id_contradiction() {
    let now = Utc::now();
    let context = context(
        now,
        OrderType::Limit,
        Some(Decimal::from(2210)),
        Some(BrokerOrderId::new("BROKER-ORDER-1")),
    );
    let mut selected = order(
        now,
        OrderStatus::Working,
        Decimal::ZERO,
        OrderType::Limit,
        Some(Decimal::from(2210)),
    );
    selected.client_order_id = Some(ClientOrderId::new("CLIENT-OTHER").unwrap());
    let result = reconcile(now, context, truth(now, vec![selected], vec![]));
    assert_eq!(result.outcome, Stage8a4OutcomeKind::Conflict);
    assert_eq!(
        result.reason,
        Stage8a4ReconciliationReason::ExactIdentityDisagreement
    );
}

#[test]
fn tier3_shape_cannot_override_explicit_client_id_contradiction() {
    let now = Utc::now();
    let mut selected = order(
        now,
        OrderStatus::Working,
        Decimal::ZERO,
        OrderType::Limit,
        Some(Decimal::from(2210)),
    );
    selected.client_order_id = Some(ClientOrderId::new("CLIENT-OTHER").unwrap());
    selected.broker_order_id = None;
    let result = reconcile(
        now,
        context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
        truth(now, vec![selected], vec![]),
    );
    assert_eq!(result.outcome, Stage8a4OutcomeKind::Conflict);
    assert_eq!(
        result.reason,
        Stage8a4ReconciliationReason::ExactIdentityDisagreement
    );
}

#[test]
fn supporting_trade_secondary_exact_identity_contradiction_is_conflict() {
    let now = Utc::now();
    for mut contradictory in [
        trade(now, "TRADE-CLIENT-CONFLICT", Decimal::from(2), 3),
        trade(now, "TRADE-BROKER-CONFLICT", Decimal::from(2), 3),
    ] {
        if contradictory.broker_trade_id.as_str() == "TRADE-CLIENT-CONFLICT" {
            contradictory.client_order_id = Some(ClientOrderId::new("CLIENT-OTHER").unwrap());
        } else {
            contradictory.broker_order_id = Some(BrokerOrderId::new("BROKER-ORDER-2"));
        }
        let result = reconcile(
            now,
            context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
            truth(
                now,
                vec![order(
                    now,
                    OrderStatus::PartiallyFilled,
                    Decimal::from(2),
                    OrderType::Limit,
                    Some(Decimal::from(2210)),
                )],
                vec![contradictory],
            ),
        );
        assert_eq!(result.outcome, Stage8a4OutcomeKind::Conflict);
        assert_eq!(
            result.reason,
            Stage8a4ReconciliationReason::TradeIdentityConflict
        );
    }
}

#[test]
fn equal_material_duplicate_receipt_order_is_byte_stable() {
    let now = Utc::now();
    let first = trade(now, "TRADE-DUP", Decimal::from(2), 3);
    let mut second = first.clone();
    second.received_ts = now - Duration::seconds(2);
    let selected = order(
        now,
        OrderStatus::PartiallyFilled,
        Decimal::from(2),
        OrderType::Limit,
        Some(Decimal::from(2210)),
    );
    let left = reconcile(
        now,
        context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
        truth(
            now,
            vec![selected.clone()],
            vec![first.clone(), second.clone()],
        ),
    );
    let right = reconcile(
        now,
        context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
        truth(now, vec![selected], vec![second, first]),
    );
    assert_eq!(
        serde_json::to_vec(&left).unwrap(),
        serde_json::to_vec(&right).unwrap()
    );
    assert_eq!(
        left.trade_summary_binding_sha256,
        right.trade_summary_binding_sha256
    );
    assert_eq!(left.semantic_binding_sha256, right.semantic_binding_sha256);
}

#[test]
fn identical_context_policy_truth_tuple_replays_byte_stably() {
    let now = Utc::now();
    let run = || {
        reconcile(
            now,
            context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
            truth(
                now,
                vec![order(
                    now,
                    OrderStatus::PartiallyFilled,
                    Decimal::from(2),
                    OrderType::Limit,
                    Some(Decimal::from(2210)),
                )],
                vec![trade(now, "TRADE-REPLAY", Decimal::from(2), 3)],
            ),
        )
    };
    assert_eq!(
        serde_json::to_vec(&run()).unwrap(),
        serde_json::to_vec(&run()).unwrap()
    );
}

#[test]
fn non_exact_diagnostic_is_byte_stable_under_canonical_row_reordering() {
    let now = Utc::now();
    let first = order(
        now,
        OrderStatus::Working,
        Decimal::ZERO,
        OrderType::Limit,
        Some(Decimal::from(2210)),
    );
    let mut second = first.clone();
    second.broker_order_id = Some(BrokerOrderId::new("BROKER-ORDER-2"));
    let left = reconcile(
        now,
        context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
        truth(now, vec![first.clone(), second.clone()], vec![]),
    );
    let right = reconcile(
        now,
        context(now, OrderType::Limit, Some(Decimal::from(2210)), None),
        truth(now, vec![second, first], vec![]),
    );
    assert_eq!(left.outcome, Stage8a4OutcomeKind::Conflict);
    assert_eq!(
        serde_json::to_vec(&left).unwrap(),
        serde_json::to_vec(&right).unwrap()
    );
}
