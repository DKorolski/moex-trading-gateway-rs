//! Stage 8A-2 existing-builder composition behind an in-memory no-send sink.
//!
//! The only production input is the freshly revalidated Stage 8A-1
//! continuation. It is consumed by value and cannot be reused.
//!
//! ```compile_fail
//! use finam_gateway::{
//!     Stage8a1CurrentlyAuthorizedCapability, Stage8a2InMemoryNoSendSink,
//! };
//! use broker_finam::FinamPlaceOrderRequestSpec;
//! fn leak_raw(
//!     capability: Stage8a1CurrentlyAuthorizedCapability,
//!     sink: &mut Stage8a2InMemoryNoSendSink,
//! ) -> FinamPlaceOrderRequestSpec {
//!     capability.compose_stage8a2_no_send(sink).unwrap()
//! }
//! ```
//!
//! ```compile_fail
//! use finam_gateway::{
//!     Stage8a1CurrentlyAuthorizedCapability, Stage8a2InMemoryNoSendSink,
//! };
//! fn extract(
//!     capability: Stage8a1CurrentlyAuthorizedCapability,
//!     sink: &mut Stage8a2InMemoryNoSendSink,
//! ) {
//!     let diagnostic = capability.compose_stage8a2_no_send(sink).unwrap();
//!     let _ = diagnostic.raw_request();
//! }
//! ```
//!
//! ```compile_fail
//! use finam_gateway::{
//!     Stage8a1CurrentlyAuthorizedCapability, Stage8a2InMemoryNoSendSink,
//! };
//! fn reuse(
//!     capability: Stage8a1CurrentlyAuthorizedCapability,
//!     first: &mut Stage8a2InMemoryNoSendSink,
//!     second: &mut Stage8a2InMemoryNoSendSink,
//! ) {
//!     capability.compose_stage8a2_no_send(first).unwrap();
//!     capability.compose_stage8a2_no_send(second).unwrap();
//! }
//! ```

use super::{
    valid_sha256, Stage8ApprovedCommand, Stage8CommandScope, Stage8a1CurrentlyAuthorizedCapability,
};
use broker_finam::{
    build_cancel_order_request, build_place_order_request, FinamCancelOrderRequestSpec,
    FinamOrderRequestBuildError, FinamPlaceOrderRequestSpec,
};
use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8a2RequestShapeKind {
    MarketDayPlace,
    LimitDayPlace,
    Cancel,
}

/// Redacted proof that exactly one existing FINAM builder result reached the
/// deterministic no-send sink. It contains no request spec or raw identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage8a2BuilderCompositionDiagnostic {
    pub scope: Stage8CommandScope,
    pub kind: Stage8a2RequestShapeKind,
    pub request_shape_sha256: String,
    pub authority_binding_sha256: String,
    pub sink_receipt_sha256: String,
    pub sink_sequence: u64,
    pub account_id_present: bool,
    pub account_id_len: usize,
    pub symbol_present: bool,
    pub quantity_present: bool,
    pub side_present: bool,
    pub order_type_present: bool,
    pub day_time_in_force_present: bool,
    pub limit_price_present: bool,
    pub client_order_id_present: bool,
    pub broker_order_id_present: bool,
    pub comment_present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Stage8a2BuilderCompositionError {
    #[error("Stage 8A-2 continuation is expired or internally inconsistent")]
    ContinuationInvalid,
    #[error("Stage 8A-2 existing FINAM builder rejected the approved command: {0}")]
    ExistingBuilder(#[from] FinamOrderRequestBuildError),
    #[error("Stage 8A-2 request-shape encoding failed")]
    ShapeEncoding,
    #[error("Stage 8A-2 no-send sink sequence is exhausted")]
    SinkSequenceExhausted,
}

/// Deterministic sink with no transport, URL, token or broker outcome surface.
/// It intentionally has no Clone, Debug or Serialize implementation.
#[derive(Default)]
pub struct Stage8a2InMemoryNoSendSink {
    consumed_count: u64,
    last_receipt_sha256: Option<String>,
}

impl Stage8a2InMemoryNoSendSink {
    pub fn new() -> Self {
        Self::default()
    }

    fn consume(
        &mut self,
        witness: Stage8a2OpaqueRequestShapeWitness,
    ) -> Result<Stage8a2BuilderCompositionDiagnostic, Stage8a2BuilderCompositionError> {
        let sequence = self
            .consumed_count
            .checked_add(1)
            .ok_or(Stage8a2BuilderCompositionError::SinkSequenceExhausted)?;
        let receipt_sha256 = digest_parts(
            b"stage8a2-in-memory-no-send-receipt-v1",
            &[
                witness.request_shape_sha256.as_bytes(),
                witness.authority_binding_sha256.as_bytes(),
                &sequence.to_be_bytes(),
            ],
        );
        self.consumed_count = sequence;
        self.last_receipt_sha256 = Some(receipt_sha256.clone());
        Ok(witness.into_diagnostic(sequence, receipt_sha256))
    }

    #[cfg(test)]
    fn consumed_count(&self) -> u64 {
        self.consumed_count
    }
}

/// Private non-serializable/non-debug request-shape witness. Raw builder
/// output is reduced to this witness in the Stage 8A-1 privacy domain.
struct Stage8a2OpaqueRequestShapeWitness {
    scope: Stage8CommandScope,
    kind: Stage8a2RequestShapeKind,
    request_shape_sha256: String,
    authority_binding_sha256: String,
    account_id_present: bool,
    account_id_len: usize,
    symbol_present: bool,
    quantity_present: bool,
    side_present: bool,
    order_type_present: bool,
    day_time_in_force_present: bool,
    limit_price_present: bool,
    client_order_id_present: bool,
    broker_order_id_present: bool,
    comment_present: bool,
}

impl Stage8a2OpaqueRequestShapeWitness {
    fn into_diagnostic(
        self,
        sink_sequence: u64,
        sink_receipt_sha256: String,
    ) -> Stage8a2BuilderCompositionDiagnostic {
        Stage8a2BuilderCompositionDiagnostic {
            scope: self.scope,
            kind: self.kind,
            request_shape_sha256: self.request_shape_sha256,
            authority_binding_sha256: self.authority_binding_sha256,
            sink_receipt_sha256,
            sink_sequence,
            account_id_present: self.account_id_present,
            account_id_len: self.account_id_len,
            symbol_present: self.symbol_present,
            quantity_present: self.quantity_present,
            side_present: self.side_present,
            order_type_present: self.order_type_present,
            day_time_in_force_present: self.day_time_in_force_present,
            limit_price_present: self.limit_price_present,
            client_order_id_present: self.client_order_id_present,
            broker_order_id_present: self.broker_order_id_present,
            comment_present: self.comment_present,
        }
    }
}

impl Stage8a1CurrentlyAuthorizedCapability {
    /// The single Stage 8A-2 production composition seam. `self` is consumed;
    /// neither the approved command nor raw FINAM request spec is returned.
    pub fn compose_stage8a2_no_send(
        self,
        sink: &mut Stage8a2InMemoryNoSendSink,
    ) -> Result<Stage8a2BuilderCompositionDiagnostic, Stage8a2BuilderCompositionError> {
        let Stage8a1CurrentlyAuthorizedCapability {
            capability,
            revalidated_at,
            current_state_sha256,
        } = self;
        let now = Utc::now();
        if revalidated_at > now
            || now >= capability.valid_until
            || current_state_sha256 != capability.current_state_sha256
            || !valid_sha256(&current_state_sha256)
        {
            return Err(Stage8a2BuilderCompositionError::ContinuationInvalid);
        }

        let authority_binding_sha256 = digest_parts(
            b"stage8a2-fresh-continuation-binding-v1",
            &[
                capability.audit_fingerprint.as_bytes(),
                current_state_sha256.as_bytes(),
                revalidated_at.to_rfc3339().as_bytes(),
                capability.valid_until.to_rfc3339().as_bytes(),
            ],
        );
        let scope = capability.scope;
        let request_id = capability.request_id;
        let witness = match (scope, capability.approved) {
            (Stage8CommandScope::Place, Stage8ApprovedCommand::Place(approved)) => {
                if approved.order().request_id != request_id {
                    return Err(Stage8a2BuilderCompositionError::ContinuationInvalid);
                }
                // Stage 8A-2 has no outgoing-comment authority. `None` is the
                // only accepted command-to-wire composition.
                let spec = build_place_order_request(&approved, None)?;
                place_witness(spec, authority_binding_sha256)?
            }
            (Stage8CommandScope::Cancel, Stage8ApprovedCommand::Cancel(approved)) => {
                if approved.cancel().request_id != request_id {
                    return Err(Stage8a2BuilderCompositionError::ContinuationInvalid);
                }
                let spec = build_cancel_order_request(&approved)?;
                cancel_witness(spec, authority_binding_sha256)
            }
            _ => return Err(Stage8a2BuilderCompositionError::ContinuationInvalid),
        };
        sink.consume(witness)
    }
}

fn place_witness(
    spec: FinamPlaceOrderRequestSpec,
    authority_binding_sha256: String,
) -> Result<Stage8a2OpaqueRequestShapeWitness, Stage8a2BuilderCompositionError> {
    let path = spec.redacted_path_shape();
    let body = spec.redacted_body_shape();
    let request_shape_sha256 = place_request_shape_sha256(&spec)?;
    let kind = if spec.body.limit_price.is_some() {
        Stage8a2RequestShapeKind::LimitDayPlace
    } else {
        Stage8a2RequestShapeKind::MarketDayPlace
    };
    Ok(Stage8a2OpaqueRequestShapeWitness {
        scope: Stage8CommandScope::Place,
        kind,
        request_shape_sha256,
        authority_binding_sha256,
        account_id_present: path.account_id_present,
        account_id_len: path.account_id_len,
        symbol_present: body.symbol_present,
        quantity_present: body.quantity_present,
        side_present: body.side_present,
        order_type_present: body.order_type_present,
        day_time_in_force_present: spec.body.time_in_force.as_deref() == Some("TIME_IN_FORCE_DAY"),
        limit_price_present: body.limit_price_present,
        client_order_id_present: body.client_order_id_present,
        broker_order_id_present: false,
        comment_present: body.comment_present,
    })
}

fn cancel_witness(
    spec: FinamCancelOrderRequestSpec,
    authority_binding_sha256: String,
) -> Stage8a2OpaqueRequestShapeWitness {
    let path = spec.redacted_path_shape();
    Stage8a2OpaqueRequestShapeWitness {
        scope: Stage8CommandScope::Cancel,
        kind: Stage8a2RequestShapeKind::Cancel,
        request_shape_sha256: cancel_request_shape_sha256(&spec),
        authority_binding_sha256,
        account_id_present: path.account_id_present,
        account_id_len: path.account_id_len,
        symbol_present: false,
        quantity_present: false,
        side_present: false,
        order_type_present: false,
        day_time_in_force_present: false,
        limit_price_present: false,
        client_order_id_present: false,
        broker_order_id_present: path.order_id_present,
        comment_present: false,
    }
}

fn place_request_shape_sha256(
    spec: &FinamPlaceOrderRequestSpec,
) -> Result<String, Stage8a2BuilderCompositionError> {
    let body = serde_json::to_vec(&spec.body)
        .map_err(|_| Stage8a2BuilderCompositionError::ShapeEncoding)?;
    Ok(digest_parts(
        b"stage8a2-existing-place-builder-shape-v1",
        &[spec.account_id.as_bytes(), &body],
    ))
}

fn cancel_request_shape_sha256(spec: &FinamCancelOrderRequestSpec) -> String {
    digest_parts(
        b"stage8a2-existing-cancel-builder-shape-v1",
        &[spec.account_id.as_bytes(), spec.order_id.as_bytes()],
    )
}

fn digest_parts(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update((domain.len() as u64).to_be_bytes());
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    let digest = hasher.finalize();
    let mut output = String::with_capacity(digest.len() * 2);
    use std::fmt::Write as _;
    for byte in digest {
        write!(&mut output, "{byte:02x}").expect("hex formatting cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::super::Stage8ExecutionCapability;
    use super::*;
    use broker_core::{
        AccountId, BrokerOrderId, CancelOrder, CancelPreflightApproval, ClientOrderId, Exchange,
        InstrumentId, Market, OperatorArm, OrderPathRecord, OrderPathState, OrderPreflightContext,
        OrderPreflightPolicy, OrderReferencePrice, OrderSide, OrderType, PlaceOrder,
        StrategyRequestId, TimeInForce,
    };
    use chrono::{Duration, Utc};
    use rust_decimal::Decimal;
    use uuid::Uuid;

    const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn request_id(value: u128) -> StrategyRequestId {
        StrategyRequestId::from(Uuid::from_u128(value))
    }

    fn account() -> AccountId {
        AccountId::new("ACC_TEST_0001")
    }

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".into(),
            venue_symbol: Some("IMOEXF@RTSX".into()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn policy(now: chrono::DateTime<Utc>) -> OrderPreflightPolicy {
        OrderPreflightPolicy {
            allowed_accounts: vec![account()],
            allowed_venue_symbols: vec!["IMOEXF@RTSX".into()],
            allowed_order_types: vec![OrderType::Market, OrderType::Limit],
            allowed_time_in_force: vec![TimeInForce::Day],
            min_qty: Decimal::ONE,
            qty_step: Decimal::ONE,
            max_qty: Decimal::new(2, 0),
            price_step: Some(Decimal::new(5, 1)),
            max_market_qty: Decimal::ONE,
            max_notional_per_order: Some(Decimal::new(5_000, 0)),
            max_notional_per_run: Some(Decimal::new(5_000, 0)),
            max_limit_deviation_bps: Some(1_000),
            max_reference_age_ms: 5_000,
            allow_cancel_by_broker_order_id_without_mapping: false,
            operator_arm: OperatorArm {
                session_id: "SESSION_TEST".into(),
                armed_until: now + Duration::minutes(1),
                endpoint_calls_enabled: true,
                one_shot: true,
                endpoint_attempted: false,
                preflight_digest: HASH_A.into(),
            },
        }
    }

    fn place(kind: OrderType, value: u128) -> PlaceOrder {
        let now = Utc::now();
        let request_id = request_id(value);
        PlaceOrder {
            request_id,
            created_ts: now - Duration::seconds(1),
            ttl_ms: Some(30_000),
            account_id: account(),
            client_order_id: ClientOrderId::from_strategy_request(request_id),
            instrument: instrument(),
            side: OrderSide::Buy,
            order_type: kind,
            qty: Decimal::ONE,
            limit_price: (kind == OrderType::Limit).then_some(Decimal::new(2210, 0)),
            time_in_force: TimeInForce::Day,
            comment: None,
        }
    }

    fn place_continuation(
        order: &PlaceOrder,
    ) -> (
        Stage8a1CurrentlyAuthorizedCapability,
        broker_core::PreflightApprovedPlaceOrder,
    ) {
        let now = Utc::now();
        let context = OrderPreflightContext {
            reference_price: Some(OrderReferencePrice {
                price: Decimal::new(2220, 0),
                received_ts: now,
            }),
            current_run_notional: Decimal::ZERO,
        };
        let approved = policy(now)
            .approve_place_order_with_context(order, now, &context)
            .unwrap();
        (
            continuation(
                Stage8ApprovedCommand::Place(approved.clone()),
                Stage8CommandScope::Place,
                order.request_id,
                now,
            ),
            approved,
        )
    }

    fn cancel_continuation() -> (
        Stage8a1CurrentlyAuthorizedCapability,
        broker_core::PreflightApprovedCancelOrder,
    ) {
        let now = Utc::now();
        let placed = place(OrderType::Limit, 20);
        let mut existing = OrderPathRecord::from_place_order(&placed, now, None);
        existing.broker_order_id = Some(BrokerOrderId::new("BROKER_TEST_STRING_1"));
        existing.state = OrderPathState::Submitted;
        let cancel = CancelOrder {
            request_id: request_id(21),
            created_ts: now - Duration::seconds(1),
            ttl_ms: Some(30_000),
            account_id: account(),
            order_id: existing.broker_order_id.clone().unwrap(),
            client_order_id: Some(placed.client_order_id.clone()),
        };
        let approved = match policy(now)
            .approve_cancel_order(&cancel, now, Some(&existing))
            .unwrap()
        {
            CancelPreflightApproval::Submit(approved) => approved,
            CancelPreflightApproval::AlreadyTerminal => panic!("working order expected"),
        };
        (
            continuation(
                Stage8ApprovedCommand::Cancel(approved.clone()),
                Stage8CommandScope::Cancel,
                cancel.request_id,
                now,
            ),
            approved,
        )
    }

    fn continuation(
        approved: Stage8ApprovedCommand,
        scope: Stage8CommandScope,
        request_id: StrategyRequestId,
        now: chrono::DateTime<Utc>,
    ) -> Stage8a1CurrentlyAuthorizedCapability {
        Stage8a1CurrentlyAuthorizedCapability {
            capability: Stage8ExecutionCapability {
                approved,
                scope,
                request_id,
                issued_at: now,
                valid_until: now + Duration::seconds(30),
                seal_generation: 7,
                durable_provenance_sha256: HASH_A.into(),
                seal_commitment_sha256: HASH_B.into(),
                policy_sha256: HASH_A.into(),
                build_sha256: HASH_B.into(),
                config_sha256: HASH_A.into(),
                endpoint_policy_sha256: HASH_B.into(),
                authority_scope_sha256: HASH_A.into(),
                arm_nonce_sha256: HASH_B.into(),
                exact_command_sha256: HASH_A.into(),
                current_state_sha256: HASH_B.into(),
                audit_fingerprint: HASH_A.into(),
            },
            revalidated_at: now,
            current_state_sha256: HASH_B.into(),
        }
    }

    #[test]
    fn market_day_matches_existing_builder_and_records_once() {
        let order = place(OrderType::Market, 1);
        let (continuation, approved) = place_continuation(&order);
        let expected = build_place_order_request(&approved, None).unwrap();
        let expected_hash = place_request_shape_sha256(&expected).unwrap();
        let mut sink = Stage8a2InMemoryNoSendSink::new();

        let diagnostic = continuation.compose_stage8a2_no_send(&mut sink).unwrap();

        assert_eq!(diagnostic.kind, Stage8a2RequestShapeKind::MarketDayPlace);
        assert_eq!(diagnostic.request_shape_sha256, expected_hash);
        assert!(diagnostic.day_time_in_force_present);
        assert!(!diagnostic.limit_price_present);
        assert!(diagnostic.client_order_id_present);
        assert!(!diagnostic.comment_present);
        assert_eq!(diagnostic.sink_sequence, 1);
        assert_eq!(sink.consumed_count(), 1);
    }

    #[test]
    fn limit_day_matches_existing_builder_with_exact_client_identity() {
        let order = place(OrderType::Limit, 2);
        let expected_client_order_id = order.client_order_id.as_str().to_string();
        let (continuation, approved) = place_continuation(&order);
        let expected = build_place_order_request(&approved, None).unwrap();
        assert_eq!(
            expected.body.client_order_id.as_deref(),
            Some(expected_client_order_id.as_str())
        );
        let expected_hash = place_request_shape_sha256(&expected).unwrap();
        let mut sink = Stage8a2InMemoryNoSendSink::new();

        let diagnostic = continuation.compose_stage8a2_no_send(&mut sink).unwrap();

        assert_eq!(diagnostic.kind, Stage8a2RequestShapeKind::LimitDayPlace);
        assert_eq!(diagnostic.request_shape_sha256, expected_hash);
        assert!(diagnostic.limit_price_present);
        assert!(!diagnostic.comment_present);
        assert_eq!(sink.consumed_count(), 1);
    }

    #[test]
    fn cancel_matches_existing_builder_and_preserves_string_order_id() {
        let (continuation, approved) = cancel_continuation();
        let expected = build_cancel_order_request(&approved).unwrap();
        assert_eq!(expected.order_id, "BROKER_TEST_STRING_1");
        let expected_hash = cancel_request_shape_sha256(&expected);
        let mut sink = Stage8a2InMemoryNoSendSink::new();

        let diagnostic = continuation.compose_stage8a2_no_send(&mut sink).unwrap();

        assert_eq!(diagnostic.kind, Stage8a2RequestShapeKind::Cancel);
        assert_eq!(diagnostic.request_shape_sha256, expected_hash);
        assert!(diagnostic.broker_order_id_present);
        assert_eq!(sink.consumed_count(), 1);
    }

    #[test]
    fn invalid_continuation_has_zero_sink_effect() {
        let order = place(OrderType::Market, 3);
        let (mut continuation, _) = place_continuation(&order);
        continuation.current_state_sha256 = HASH_A.into();
        let mut sink = Stage8a2InMemoryNoSendSink::new();

        assert_eq!(
            continuation.compose_stage8a2_no_send(&mut sink),
            Err(Stage8a2BuilderCompositionError::ContinuationInvalid)
        );
        assert_eq!(sink.consumed_count(), 0);
    }

    #[test]
    fn malformed_order_never_reaches_composition_sink() {
        let now = Utc::now();
        let mut malformed = place(OrderType::Market, 4);
        malformed.instrument.venue_symbol = None;
        let sink = Stage8a2InMemoryNoSendSink::new();

        assert!(policy(now).approve_place_order(&malformed, now).is_err());
        assert_eq!(sink.consumed_count(), 0);
    }
}
