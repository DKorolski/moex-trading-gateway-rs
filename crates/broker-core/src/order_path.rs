use std::collections::HashMap;
use std::fmt::Write;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::account::AccountId;
use crate::command::{CommandAckStatus, PlaceOrder};
use crate::ids::{BrokerOrderId, ClientOrderId, StrategyRequestId};
use crate::instrument::{InstrumentId, Price, Quantity};
use crate::order::{OrderSide, OrderType, RedactedValueFingerprint, TimeInForce};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderPathCommandKind {
    Place,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderPathState {
    IntentRecorded,
    LocalRejected,
    SubmitInFlight,
    Submitted,
    TimeoutUnknownPending,
    RecoveredByClientOrderId,
    BrokerRejected,
    CancelRequested,
    CancelSubmitted,
    Terminal,
    ManualInterventionRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderPathEvent {
    LocalReject,
    BeginSubmit,
    SubmitAccepted,
    SubmitTimedOut,
    RecoverByClientOrderId,
    RequireManualIntervention,
    BrokerReject,
    RequestCancel,
    CancelAccepted,
    MarkTerminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderPathErrorKind {
    LocalValidation,
    BrokerRejected,
    TransportTimeout,
    RateLimited,
    ReconciliationRequired,
    DurableStoreUnavailable,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderPathReconciliationSource {
    ClientOrderId,
    BrokerOrderId,
    OrderSnapshot,
    TradeSnapshot,
    ManualOperator,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderPathRecord {
    pub request_id: StrategyRequestId,
    pub client_order_id: ClientOrderId,
    pub broker_order_id: Option<BrokerOrderId>,
    pub command_kind: OrderPathCommandKind,
    pub account_id: AccountId,
    pub instrument: InstrumentId,
    pub side: Option<OrderSide>,
    pub order_type: Option<OrderType>,
    pub qty: Option<Quantity>,
    pub limit_price: Option<Price>,
    pub time_in_force: Option<TimeInForce>,
    pub created_ts: DateTime<Utc>,
    pub last_update_ts: DateTime<Utc>,
    pub submit_attempt_count: u32,
    pub cancel_attempt_count: u32,
    pub state: OrderPathState,
    pub last_ack_status: Option<CommandAckStatus>,
    pub last_error_kind: Option<OrderPathErrorKind>,
    pub last_reconciliation_source: Option<OrderPathReconciliationSource>,
    pub outgoing_comment_fingerprint: Option<RedactedValueFingerprint>,
}

impl OrderPathRecord {
    pub fn from_place_order(
        order: &PlaceOrder,
        now: DateTime<Utc>,
        outgoing_comment: Option<&OutgoingOrderComment>,
    ) -> Self {
        Self {
            request_id: order.request_id,
            client_order_id: order.client_order_id.clone(),
            broker_order_id: None,
            command_kind: OrderPathCommandKind::Place,
            account_id: order.account_id.clone(),
            instrument: order.instrument.clone(),
            side: Some(order.side),
            order_type: Some(order.order_type),
            qty: Some(order.qty),
            limit_price: order.limit_price,
            time_in_force: Some(order.time_in_force),
            created_ts: order.created_ts,
            last_update_ts: now,
            submit_attempt_count: 0,
            cancel_attempt_count: 0,
            state: OrderPathState::IntentRecorded,
            last_ack_status: None,
            last_error_kind: None,
            last_reconciliation_source: None,
            outgoing_comment_fingerprint: outgoing_comment.map(|comment| comment.fingerprint()),
        }
    }

    pub fn transition(
        &mut self,
        event: OrderPathEvent,
        now: DateTime<Utc>,
    ) -> Result<(), OrderPathTransitionError> {
        let next = next_order_path_state(self.state, event).ok_or(
            OrderPathTransitionError::InvalidTransition {
                from: self.state,
                event,
            },
        )?;
        match event {
            OrderPathEvent::BeginSubmit => {
                self.submit_attempt_count += 1;
                self.last_ack_status = Some(CommandAckStatus::Accepted);
            }
            OrderPathEvent::SubmitAccepted => {
                self.last_ack_status = Some(CommandAckStatus::Submitted);
            }
            OrderPathEvent::SubmitTimedOut => {
                self.last_ack_status = Some(CommandAckStatus::Timeout);
                self.last_error_kind = Some(OrderPathErrorKind::TransportTimeout);
            }
            OrderPathEvent::RecoverByClientOrderId => {
                self.last_ack_status = Some(CommandAckStatus::Recovered);
                self.last_reconciliation_source =
                    Some(OrderPathReconciliationSource::ClientOrderId);
            }
            OrderPathEvent::LocalReject | OrderPathEvent::BrokerReject => {
                self.last_ack_status = Some(CommandAckStatus::Rejected);
                self.last_error_kind = Some(match event {
                    OrderPathEvent::LocalReject => OrderPathErrorKind::LocalValidation,
                    OrderPathEvent::BrokerReject => OrderPathErrorKind::BrokerRejected,
                    _ => OrderPathErrorKind::Unknown,
                });
            }
            OrderPathEvent::RequestCancel => {
                self.cancel_attempt_count += 1;
                self.last_ack_status = Some(CommandAckStatus::Accepted);
            }
            OrderPathEvent::CancelAccepted => {
                self.last_ack_status = Some(CommandAckStatus::Submitted);
            }
            OrderPathEvent::RequireManualIntervention => {
                self.last_ack_status = Some(CommandAckStatus::UnknownPending);
                self.last_error_kind = Some(OrderPathErrorKind::ReconciliationRequired);
            }
            OrderPathEvent::MarkTerminal => {}
        }
        self.state = next;
        self.last_update_ts = now;
        Ok(())
    }

    pub fn recover_after_restart(
        &mut self,
        now: DateTime<Utc>,
    ) -> Result<(), OrderPathTransitionError> {
        if self.state == OrderPathState::SubmitInFlight {
            self.transition(OrderPathEvent::SubmitTimedOut, now)?;
        }
        Ok(())
    }
}

fn next_order_path_state(state: OrderPathState, event: OrderPathEvent) -> Option<OrderPathState> {
    use OrderPathEvent as E;
    use OrderPathState as S;
    match (state, event) {
        (S::IntentRecorded, E::LocalReject) => Some(S::LocalRejected),
        (S::IntentRecorded, E::BeginSubmit) => Some(S::SubmitInFlight),
        (S::SubmitInFlight, E::SubmitAccepted) => Some(S::Submitted),
        (S::SubmitInFlight, E::SubmitTimedOut) => Some(S::TimeoutUnknownPending),
        (S::TimeoutUnknownPending, E::RecoverByClientOrderId) => Some(S::RecoveredByClientOrderId),
        (S::TimeoutUnknownPending, E::RequireManualIntervention) => {
            Some(S::ManualInterventionRequired)
        }
        (S::SubmitInFlight, E::BrokerReject) => Some(S::BrokerRejected),
        (S::Submitted, E::BrokerReject) => Some(S::BrokerRejected),
        (S::Submitted, E::RequestCancel) => Some(S::CancelRequested),
        (S::CancelRequested, E::CancelAccepted) => Some(S::CancelSubmitted),
        (S::Submitted, E::MarkTerminal)
        | (S::RecoveredByClientOrderId, E::MarkTerminal)
        | (S::CancelRequested, E::MarkTerminal)
        | (S::CancelSubmitted, E::MarkTerminal)
        | (S::BrokerRejected, E::MarkTerminal)
        | (S::LocalRejected, E::MarkTerminal) => Some(S::Terminal),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OrderPathTransitionError {
    #[error("invalid order-path transition from {from:?} by {event:?}")]
    InvalidTransition {
        from: OrderPathState,
        event: OrderPathEvent,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OrderPathStoreError {
    #[error("duplicate strategy request id: {0}")]
    DuplicateStrategyRequestId(StrategyRequestId),
    #[error("duplicate client order id: {0}")]
    DuplicateClientOrderId(ClientOrderId),
}

#[derive(Debug, Default)]
pub struct InMemoryOrderPathStore {
    by_request_id: HashMap<StrategyRequestId, OrderPathRecord>,
    request_by_client_id: HashMap<ClientOrderId, StrategyRequestId>,
}

impl InMemoryOrderPathStore {
    pub fn insert_intent(&mut self, record: OrderPathRecord) -> Result<(), OrderPathStoreError> {
        if self.by_request_id.contains_key(&record.request_id) {
            return Err(OrderPathStoreError::DuplicateStrategyRequestId(
                record.request_id,
            ));
        }
        if self
            .request_by_client_id
            .contains_key(&record.client_order_id)
        {
            return Err(OrderPathStoreError::DuplicateClientOrderId(
                record.client_order_id.clone(),
            ));
        }
        self.request_by_client_id
            .insert(record.client_order_id.clone(), record.request_id);
        self.by_request_id.insert(record.request_id, record);
        Ok(())
    }

    pub fn get_by_request_id(&self, request_id: StrategyRequestId) -> Option<&OrderPathRecord> {
        self.by_request_id.get(&request_id)
    }

    pub fn get_mut_by_request_id(
        &mut self,
        request_id: StrategyRequestId,
    ) -> Option<&mut OrderPathRecord> {
        self.by_request_id.get_mut(&request_id)
    }

    pub fn get_by_client_order_id(
        &self,
        client_order_id: &ClientOrderId,
    ) -> Option<&OrderPathRecord> {
        self.request_by_client_id
            .get(client_order_id)
            .and_then(|request_id| self.by_request_id.get(request_id))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CommentPolicyMode {
    Disabled,
    SanitizedDeterministic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutgoingOrderCommentPolicy {
    pub mode: CommentPolicyMode,
    pub max_len: usize,
}

impl Default for OutgoingOrderCommentPolicy {
    fn default() -> Self {
        Self {
            mode: CommentPolicyMode::Disabled,
            max_len: 64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutgoingCommentIntent<'a> {
    pub strategy_id: &'a str,
    pub intent_class: &'a str,
    pub client_order_id: &'a ClientOrderId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutgoingOrderComment {
    #[serde(default, skip_serializing, skip_deserializing)]
    value: String,
    fingerprint: RedactedValueFingerprint,
}

impl OutgoingOrderComment {
    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn fingerprint(&self) -> RedactedValueFingerprint {
        self.fingerprint.clone()
    }
}

impl OutgoingOrderCommentPolicy {
    pub fn build(
        &self,
        intent: OutgoingCommentIntent<'_>,
    ) -> Result<Option<OutgoingOrderComment>, OutgoingCommentError> {
        match self.mode {
            CommentPolicyMode::Disabled => Ok(None),
            CommentPolicyMode::SanitizedDeterministic => {
                validate_comment_token(intent.strategy_id)?;
                validate_comment_token(intent.intent_class)?;
                let value = format!(
                    "strategy={};intent={};cid={}",
                    intent.strategy_id,
                    intent.intent_class,
                    intent.client_order_id.as_str()
                );
                if value.len() > self.max_len {
                    return Err(OutgoingCommentError::TooLong {
                        max: self.max_len,
                        actual: value.len(),
                    });
                }
                Ok(Some(OutgoingOrderComment {
                    fingerprint: fingerprint_redacted_value(&value),
                    value,
                }))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OutgoingCommentError {
    #[error("outgoing comment token cannot be empty")]
    EmptyToken,
    #[error("outgoing comment token contains unsupported character: {0:?}")]
    UnsupportedCharacter(char),
    #[error("outgoing comment token contains forbidden word: {0}")]
    ForbiddenWord(&'static str),
    #[error("outgoing comment exceeds {max} bytes: got {actual}")]
    TooLong { max: usize, actual: usize },
}

fn validate_comment_token(value: &str) -> Result<(), OutgoingCommentError> {
    if value.is_empty() {
        return Err(OutgoingCommentError::EmptyToken);
    }
    for character in value.chars() {
        if !(character.is_ascii_alphanumeric() || character == '_' || character == '-') {
            return Err(OutgoingCommentError::UnsupportedCharacter(character));
        }
    }
    let lower = value.to_ascii_lowercase();
    for forbidden in ["secret", "jwt", "token", "account", "operator", "broker"] {
        if lower.contains(forbidden) {
            return Err(OutgoingCommentError::ForbiddenWord(forbidden));
        }
    }
    Ok(())
}

fn fingerprint_redacted_value(value: &str) -> RedactedValueFingerprint {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    let mut sha256 = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut sha256, "{byte:02x}").expect("hex write cannot fail");
    }
    RedactedValueFingerprint {
        len: value.len(),
        sha256,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperatorArm {
    pub session_id: String,
    pub armed_until: DateTime<Utc>,
    pub endpoint_calls_enabled: bool,
    pub one_shot: bool,
    pub endpoint_attempted: bool,
    pub preflight_digest: String,
}

impl OperatorArm {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<(), OrderPreflightError> {
        if !self.endpoint_calls_enabled {
            return Err(OrderPreflightError::EndpointNotArmed);
        }
        if now >= self.armed_until {
            return Err(OrderPreflightError::ArmExpired);
        }
        if self.one_shot && self.endpoint_attempted {
            return Err(OrderPreflightError::OneShotAlreadyUsed);
        }
        if self.session_id.is_empty() || self.preflight_digest.is_empty() {
            return Err(OrderPreflightError::MissingArmAudit);
        }
        Ok(())
    }

    pub fn record_endpoint_attempt(&mut self) {
        if self.one_shot {
            self.endpoint_attempted = true;
            self.endpoint_calls_enabled = false;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderPreflightPolicy {
    pub allowed_accounts: Vec<AccountId>,
    pub allowed_venue_symbols: Vec<String>,
    pub allowed_order_types: Vec<OrderType>,
    pub allowed_time_in_force: Vec<TimeInForce>,
    pub min_qty: Quantity,
    pub qty_step: Quantity,
    pub max_qty: Quantity,
    pub price_step: Option<Price>,
    pub max_market_qty: Quantity,
    pub operator_arm: OperatorArm,
}

impl OrderPreflightPolicy {
    pub fn validate_place_order(
        &self,
        order: &PlaceOrder,
        now: DateTime<Utc>,
    ) -> Result<(), OrderPreflightError> {
        self.operator_arm.validate(now)?;
        if !self.allowed_accounts.contains(&order.account_id) {
            return Err(OrderPreflightError::AccountNotAllowed);
        }
        let venue_symbol = order
            .instrument
            .venue_symbol
            .as_deref()
            .ok_or(OrderPreflightError::VenueSymbolMissing)?;
        if !self
            .allowed_venue_symbols
            .iter()
            .any(|allowed| allowed == venue_symbol)
        {
            return Err(OrderPreflightError::InstrumentNotAllowed);
        }
        if !matches!(order.order_type, OrderType::Market | OrderType::Limit) {
            return Err(OrderPreflightError::UnsupportedOrderType);
        }
        if !self.allowed_order_types.contains(&order.order_type) {
            return Err(OrderPreflightError::UnsupportedOrderType);
        }
        if !self.allowed_time_in_force.contains(&order.time_in_force) {
            return Err(OrderPreflightError::UnsupportedTimeInForce);
        }
        if order.qty <= Decimal::ZERO {
            return Err(OrderPreflightError::InvalidQuantity);
        }
        if order.qty < self.min_qty || order.qty > self.max_qty {
            return Err(OrderPreflightError::QuantityOutOfBounds);
        }
        if !is_decimal_multiple(order.qty, self.qty_step) {
            return Err(OrderPreflightError::QuantityStepMismatch);
        }
        match order.order_type {
            OrderType::Market => {
                if order.limit_price.is_some() {
                    return Err(OrderPreflightError::MarketOrderHasLimitPrice);
                }
                if order.qty > self.max_market_qty {
                    return Err(OrderPreflightError::MarketQuantityOutOfBounds);
                }
            }
            OrderType::Limit => {
                let price = order
                    .limit_price
                    .ok_or(OrderPreflightError::LimitPriceMissing)?;
                if price <= Decimal::ZERO {
                    return Err(OrderPreflightError::InvalidLimitPrice);
                }
                if let Some(step) = self.price_step {
                    if !is_decimal_multiple(price, step) {
                        return Err(OrderPreflightError::PriceStepMismatch);
                    }
                }
            }
            _ => return Err(OrderPreflightError::UnsupportedOrderType),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OrderPreflightError {
    #[error("order endpoints are not operator-armed")]
    EndpointNotArmed,
    #[error("operator arm expired")]
    ArmExpired,
    #[error("one-shot operator arm already used")]
    OneShotAlreadyUsed,
    #[error("operator arm missing session id or preflight digest")]
    MissingArmAudit,
    #[error("account is not allowlisted")]
    AccountNotAllowed,
    #[error("venue symbol is missing")]
    VenueSymbolMissing,
    #[error("instrument is not allowlisted")]
    InstrumentNotAllowed,
    #[error("order type is unsupported")]
    UnsupportedOrderType,
    #[error("time in force is unsupported")]
    UnsupportedTimeInForce,
    #[error("quantity is invalid")]
    InvalidQuantity,
    #[error("quantity is outside configured bounds")]
    QuantityOutOfBounds,
    #[error("quantity is not aligned to step")]
    QuantityStepMismatch,
    #[error("market order must not carry limit price")]
    MarketOrderHasLimitPrice,
    #[error("market order quantity exceeds configured guard")]
    MarketQuantityOutOfBounds,
    #[error("limit price is required for limit order")]
    LimitPriceMissing,
    #[error("limit price is invalid")]
    InvalidLimitPrice,
    #[error("limit price is not aligned to tick")]
    PriceStepMismatch,
}

fn is_decimal_multiple(value: Decimal, step: Decimal) -> bool {
    step > Decimal::ZERO && value % step == Decimal::ZERO
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::AccountId;
    use crate::command::PlaceOrder;
    use crate::ids::ClientOrderId;
    use crate::instrument::{Exchange, Market};
    use uuid::Uuid;

    fn request_id(n: u128) -> StrategyRequestId {
        StrategyRequestId::from(Uuid::from_u128(n))
    }

    fn account() -> AccountId {
        AccountId::new("ACC_TEST_0001")
    }

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "TESTFUT".to_string(),
            venue_symbol: Some("TESTFUT@TEST".to_string()),
            exchange: Exchange::Other("TEST".to_string()),
            market: Market::Futures,
        }
    }

    fn place_order(request: StrategyRequestId, client: &str) -> PlaceOrder {
        PlaceOrder {
            request_id: request,
            created_ts: Utc::now(),
            ttl_ms: Some(1_000),
            account_id: account(),
            client_order_id: ClientOrderId::new(client).expect("client id"),
            instrument: instrument(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            qty: Decimal::new(1, 0),
            limit_price: Some(Decimal::new(1000, 0)),
            time_in_force: TimeInForce::Day,
            comment: None,
        }
    }

    fn sample_arm(now: DateTime<Utc>) -> OperatorArm {
        OperatorArm {
            session_id: "ARM_TEST_1".to_string(),
            armed_until: now + chrono::Duration::minutes(5),
            endpoint_calls_enabled: true,
            one_shot: true,
            endpoint_attempted: false,
            preflight_digest: "digest-test".to_string(),
        }
    }

    fn preflight_policy(now: DateTime<Utc>) -> OrderPreflightPolicy {
        OrderPreflightPolicy {
            allowed_accounts: vec![account()],
            allowed_venue_symbols: vec!["TESTFUT@TEST".to_string()],
            allowed_order_types: vec![OrderType::Market, OrderType::Limit],
            allowed_time_in_force: vec![TimeInForce::Day],
            min_qty: Decimal::new(1, 0),
            qty_step: Decimal::new(1, 0),
            max_qty: Decimal::new(3, 0),
            price_step: Some(Decimal::new(5, 0)),
            max_market_qty: Decimal::new(1, 0),
            operator_arm: sample_arm(now),
        }
    }

    #[test]
    fn order_path_state_machine_follows_submit_timeout_recovery() {
        let now = Utc::now();
        let order = place_order(request_id(1), "CID000000000000001");
        let mut record = OrderPathRecord::from_place_order(&order, now, None);

        record
            .transition(OrderPathEvent::BeginSubmit, now)
            .expect("begin submit");
        assert_eq!(record.state, OrderPathState::SubmitInFlight);
        assert_eq!(record.submit_attempt_count, 1);
        record
            .transition(OrderPathEvent::SubmitTimedOut, now)
            .expect("timeout");
        assert_eq!(record.state, OrderPathState::TimeoutUnknownPending);
        assert_eq!(record.last_ack_status, Some(CommandAckStatus::Timeout));
        record
            .transition(OrderPathEvent::RecoverByClientOrderId, now)
            .expect("recover");
        assert_eq!(record.state, OrderPathState::RecoveredByClientOrderId);
        assert_eq!(record.last_ack_status, Some(CommandAckStatus::Recovered));
    }

    #[test]
    fn order_path_state_machine_rejects_terminal_resubmit() {
        let now = Utc::now();
        let order = place_order(request_id(2), "CID000000000000002");
        let mut record = OrderPathRecord::from_place_order(&order, now, None);
        record
            .transition(OrderPathEvent::LocalReject, now)
            .expect("reject");
        record
            .transition(OrderPathEvent::MarkTerminal, now)
            .expect("terminal");

        let error = record
            .transition(OrderPathEvent::BeginSubmit, now)
            .expect_err("terminal resubmit must fail");
        assert_eq!(
            error,
            OrderPathTransitionError::InvalidTransition {
                from: OrderPathState::Terminal,
                event: OrderPathEvent::BeginSubmit
            }
        );
    }

    #[test]
    fn order_path_store_rejects_duplicate_request_and_client_ids() {
        let now = Utc::now();
        let first = OrderPathRecord::from_place_order(
            &place_order(request_id(3), "CID000000000000003"),
            now,
            None,
        );
        let duplicate_request = OrderPathRecord::from_place_order(
            &place_order(request_id(3), "CID000000000000004"),
            now,
            None,
        );
        let duplicate_client = OrderPathRecord::from_place_order(
            &place_order(request_id(4), "CID000000000000003"),
            now,
            None,
        );
        let mut store = InMemoryOrderPathStore::default();

        store.insert_intent(first).expect("first insert");
        assert!(matches!(
            store.insert_intent(duplicate_request),
            Err(OrderPathStoreError::DuplicateStrategyRequestId(_))
        ));
        assert!(matches!(
            store.insert_intent(duplicate_client),
            Err(OrderPathStoreError::DuplicateClientOrderId(_))
        ));
    }

    #[test]
    fn order_path_restart_recovery_marks_inflight_as_unknown_pending() {
        let now = Utc::now();
        let order = place_order(request_id(5), "CID000000000000005");
        let mut record = OrderPathRecord::from_place_order(&order, now, None);
        record
            .transition(OrderPathEvent::BeginSubmit, now)
            .expect("begin submit");

        record
            .recover_after_restart(now + chrono::Duration::seconds(1))
            .expect("recover");

        assert_eq!(record.state, OrderPathState::TimeoutUnknownPending);
        assert_eq!(
            record.last_error_kind,
            Some(OrderPathErrorKind::TransportTimeout)
        );
    }

    #[test]
    fn order_path_restart_recovery_keeps_recorded_intent_not_submitted() {
        let now = Utc::now();
        let order = place_order(request_id(6), "CID000000000000006");
        let mut record = OrderPathRecord::from_place_order(&order, now, None);

        record
            .recover_after_restart(now + chrono::Duration::seconds(1))
            .expect("recover");

        assert_eq!(record.state, OrderPathState::IntentRecorded);
        assert_eq!(record.submit_attempt_count, 0);
    }

    #[test]
    fn outgoing_comment_policy_defaults_to_no_comment() {
        let policy = OutgoingOrderCommentPolicy::default();
        let client_order_id = ClientOrderId::new("CID000000000000007").expect("client id");
        let comment = policy
            .build(OutgoingCommentIntent {
                strategy_id: "micro",
                intent_class: "entry",
                client_order_id: &client_order_id,
            })
            .expect("comment policy");

        assert!(comment.is_none());
    }

    #[test]
    fn outgoing_comment_policy_builds_redacted_sanitized_comment() {
        let policy = OutgoingOrderCommentPolicy {
            mode: CommentPolicyMode::SanitizedDeterministic,
            max_len: 64,
        };
        let client_order_id = ClientOrderId::new("CID000000000000008").expect("client id");
        let comment = policy
            .build(OutgoingCommentIntent {
                strategy_id: "micro",
                intent_class: "entry",
                client_order_id: &client_order_id,
            })
            .expect("comment policy")
            .expect("comment enabled");

        assert_eq!(
            comment.value(),
            "strategy=micro;intent=entry;cid=CID000000000000008"
        );
        let rendered = serde_json::to_string(&comment).expect("comment serializes");
        assert!(rendered.contains("fingerprint"));
        assert!(!rendered.contains(comment.value()));
    }

    #[test]
    fn outgoing_comment_policy_rejects_unsafe_or_too_long_values() {
        let policy = OutgoingOrderCommentPolicy {
            mode: CommentPolicyMode::SanitizedDeterministic,
            max_len: 32,
        };
        let client_order_id = ClientOrderId::new("CID000000000000009").expect("client id");

        assert!(matches!(
            policy.build(OutgoingCommentIntent {
                strategy_id: "account-raw",
                intent_class: "entry",
                client_order_id: &client_order_id,
            }),
            Err(OutgoingCommentError::ForbiddenWord("account"))
        ));
        assert!(matches!(
            policy.build(OutgoingCommentIntent {
                strategy_id: "micro",
                intent_class: "entry",
                client_order_id: &client_order_id,
            }),
            Err(OutgoingCommentError::TooLong { .. })
        ));
    }

    #[test]
    fn operator_arm_ttl_and_one_shot_are_enforced() {
        let now = Utc::now();
        let mut arm = sample_arm(now);

        arm.validate(now).expect("fresh arm");
        arm.record_endpoint_attempt();
        assert!(matches!(
            arm.validate(now),
            Err(OrderPreflightError::EndpointNotArmed)
                | Err(OrderPreflightError::OneShotAlreadyUsed)
        ));

        let expired = OperatorArm {
            armed_until: now - chrono::Duration::seconds(1),
            ..sample_arm(now)
        };
        assert_eq!(
            expired.validate(now).expect_err("expired"),
            OrderPreflightError::ArmExpired
        );
    }

    #[test]
    fn preflight_accepts_valid_limit_order() {
        let now = Utc::now();
        let policy = preflight_policy(now);
        let order = place_order(request_id(10), "CID000000000000010");

        policy
            .validate_place_order(&order, now)
            .expect("valid order");
    }

    #[test]
    fn preflight_rejects_invalid_qty_price_tif_and_type_without_rounding() {
        let now = Utc::now();
        let policy = preflight_policy(now);

        let mut bad_qty = place_order(request_id(11), "CID000000000000011");
        bad_qty.qty = Decimal::new(15, 1);
        assert_eq!(
            policy.validate_place_order(&bad_qty, now).expect_err("qty"),
            OrderPreflightError::QuantityStepMismatch
        );

        let mut bad_price = place_order(request_id(12), "CID000000000000012");
        bad_price.limit_price = Some(Decimal::new(1001, 0));
        assert_eq!(
            policy
                .validate_place_order(&bad_price, now)
                .expect_err("price"),
            OrderPreflightError::PriceStepMismatch
        );

        let mut bad_tif = place_order(request_id(13), "CID000000000000013");
        bad_tif.time_in_force = TimeInForce::GoodTillCancel;
        assert_eq!(
            policy.validate_place_order(&bad_tif, now).expect_err("tif"),
            OrderPreflightError::UnsupportedTimeInForce
        );

        let mut unsupported_type = place_order(request_id(14), "CID000000000000014");
        unsupported_type.order_type = OrderType::Stop;
        assert_eq!(
            policy
                .validate_place_order(&unsupported_type, now)
                .expect_err("type"),
            OrderPreflightError::UnsupportedOrderType
        );
    }
}
