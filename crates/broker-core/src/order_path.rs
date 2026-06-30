use std::collections::HashMap;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::account::AccountId;
use crate::command::{CancelOrder, CommandAck, CommandAckStatus, PlaceOrder};
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct OrderPathStoreSnapshot {
    records: Vec<OrderPathRecord>,
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

    pub fn synthetic_ack(
        &self,
        status: CommandAckStatus,
        reason: Option<String>,
        now: DateTime<Utc>,
    ) -> CommandAck {
        CommandAck {
            request_id: self.request_id,
            client_order_id: Some(self.client_order_id.clone()),
            broker_order_id: self.broker_order_id.clone(),
            status,
            reason,
            received_ts: now,
        }
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
    #[error("order-path record not found: {0}")]
    RecordNotFound(StrategyRequestId),
    #[error("client_order_id cannot be changed for request: {0}")]
    ClientOrderIdChanged(StrategyRequestId),
    #[error("order-path store io error: {0}")]
    Io(String),
    #[error("order-path store serialization error: {0}")]
    Serialization(String),
}

pub trait OrderPathStore {
    fn insert_intent(&mut self, record: OrderPathRecord) -> Result<(), OrderPathStoreError>;
    fn load_by_request_id(&self, request_id: StrategyRequestId) -> Option<OrderPathRecord>;
    fn load_by_client_order_id(&self, client_order_id: &ClientOrderId) -> Option<OrderPathRecord>;
    fn update_record(&mut self, record: OrderPathRecord) -> Result<(), OrderPathStoreError>;
    fn all_records(&self) -> Vec<OrderPathRecord>;
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

    pub fn update_record(&mut self, record: OrderPathRecord) -> Result<(), OrderPathStoreError> {
        let existing = self
            .by_request_id
            .get(&record.request_id)
            .ok_or(OrderPathStoreError::RecordNotFound(record.request_id))?;
        if existing.client_order_id != record.client_order_id {
            return Err(OrderPathStoreError::ClientOrderIdChanged(record.request_id));
        }
        self.by_request_id.insert(record.request_id, record);
        Ok(())
    }

    pub fn all_records(&self) -> Vec<OrderPathRecord> {
        self.by_request_id.values().cloned().collect()
    }
}

impl OrderPathStore for InMemoryOrderPathStore {
    fn insert_intent(&mut self, record: OrderPathRecord) -> Result<(), OrderPathStoreError> {
        InMemoryOrderPathStore::insert_intent(self, record)
    }

    fn load_by_request_id(&self, request_id: StrategyRequestId) -> Option<OrderPathRecord> {
        self.get_by_request_id(request_id).cloned()
    }

    fn load_by_client_order_id(&self, client_order_id: &ClientOrderId) -> Option<OrderPathRecord> {
        self.get_by_client_order_id(client_order_id).cloned()
    }

    fn update_record(&mut self, record: OrderPathRecord) -> Result<(), OrderPathStoreError> {
        InMemoryOrderPathStore::update_record(self, record)
    }

    fn all_records(&self) -> Vec<OrderPathRecord> {
        InMemoryOrderPathStore::all_records(self)
    }
}

#[derive(Debug)]
pub struct JsonFileOrderPathStore {
    path: PathBuf,
    inner: InMemoryOrderPathStore,
}

impl JsonFileOrderPathStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, OrderPathStoreError> {
        let path = path.into();
        let inner = if path.exists() {
            let raw = fs::read_to_string(&path)
                .map_err(|error| OrderPathStoreError::Io(error.to_string()))?;
            if raw.trim().is_empty() {
                InMemoryOrderPathStore::default()
            } else {
                let snapshot: OrderPathStoreSnapshot = serde_json::from_str(&raw)
                    .map_err(|error| OrderPathStoreError::Serialization(error.to_string()))?;
                let mut inner = InMemoryOrderPathStore::default();
                for record in snapshot.records {
                    inner.insert_intent(record)?;
                }
                inner
            }
        } else {
            InMemoryOrderPathStore::default()
        };
        Ok(Self { path, inner })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn persist(&self) -> Result<(), OrderPathStoreError> {
        if let Some(parent) = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)
                .map_err(|error| OrderPathStoreError::Io(error.to_string()))?;
        }
        let mut records = self.inner.all_records();
        records.sort_by(|left, right| {
            left.request_id
                .to_string()
                .cmp(&right.request_id.to_string())
        });
        let snapshot = OrderPathStoreSnapshot { records };
        let raw = serde_json::to_string_pretty(&snapshot)
            .map_err(|error| OrderPathStoreError::Serialization(error.to_string()))?;
        let mut tmp = self.path.clone();
        tmp.set_extension("tmp");
        fs::write(&tmp, raw).map_err(|error| OrderPathStoreError::Io(error.to_string()))?;
        fs::rename(&tmp, &self.path).map_err(|error| OrderPathStoreError::Io(error.to_string()))?;
        Ok(())
    }
}

impl OrderPathStore for JsonFileOrderPathStore {
    fn insert_intent(&mut self, record: OrderPathRecord) -> Result<(), OrderPathStoreError> {
        self.inner.insert_intent(record)?;
        self.persist()
    }

    fn load_by_request_id(&self, request_id: StrategyRequestId) -> Option<OrderPathRecord> {
        self.inner.load_by_request_id(request_id)
    }

    fn load_by_client_order_id(&self, client_order_id: &ClientOrderId) -> Option<OrderPathRecord> {
        self.inner.load_by_client_order_id(client_order_id)
    }

    fn update_record(&mut self, record: OrderPathRecord) -> Result<(), OrderPathStoreError> {
        self.inner.update_record(record)?;
        self.persist()
    }

    fn all_records(&self) -> Vec<OrderPathRecord> {
        self.inner.all_records()
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
    pub max_notional_per_order: Option<Decimal>,
    pub max_notional_per_run: Option<Decimal>,
    pub max_limit_deviation_bps: Option<u32>,
    pub max_reference_age_ms: u64,
    pub allow_cancel_by_broker_order_id_without_mapping: bool,
    pub operator_arm: OperatorArm,
}

impl OrderPreflightPolicy {
    pub fn validate_place_order(
        &self,
        order: &PlaceOrder,
        now: DateTime<Utc>,
    ) -> Result<(), OrderPreflightError> {
        self.validate_place_order_with_context(order, now, &OrderPreflightContext::default())
    }

    pub fn validate_place_order_with_context(
        &self,
        order: &PlaceOrder,
        now: DateTime<Utc>,
        context: &OrderPreflightContext,
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
                let reference = self.require_fresh_reference(context, now)?;
                self.check_notional(order.qty * reference.price, context.current_run_notional)?;
            }
            OrderType::Limit => {
                let price = order
                    .limit_price
                    .ok_or(OrderPreflightError::LimitPriceMissing)?;
                if price <= Decimal::ZERO {
                    return Err(OrderPreflightError::InvalidLimitPrice);
                }
                let step = self
                    .price_step
                    .ok_or(OrderPreflightError::ReferenceDataNotLoaded)?;
                if !is_decimal_multiple(price, step) {
                    return Err(OrderPreflightError::PriceStepMismatch);
                }
                if self.max_limit_deviation_bps.is_some() {
                    let reference = self.require_fresh_reference(context, now)?;
                    self.check_limit_deviation(price, reference.price)?;
                }
                self.check_notional(order.qty * price, context.current_run_notional)?;
            }
            _ => return Err(OrderPreflightError::UnsupportedOrderType),
        }
        Ok(())
    }

    pub fn validate_cancel_order(
        &self,
        cancel: &CancelOrder,
        now: DateTime<Utc>,
        existing: Option<&OrderPathRecord>,
    ) -> Result<CancelPreflightDecision, OrderPreflightError> {
        self.operator_arm.validate(now)?;
        if !self.allowed_accounts.contains(&cancel.account_id) {
            return Err(OrderPreflightError::AccountNotAllowed);
        }
        if cancel.order_id.as_str().is_empty() {
            return Err(OrderPreflightError::BrokerOrderIdMissing);
        }
        if let Some(record) = existing {
            if record.account_id != cancel.account_id {
                return Err(OrderPreflightError::AccountNotAllowed);
            }
            if is_terminal_order_path_state(record.state) {
                return Ok(CancelPreflightDecision::AlreadyTerminal);
            }
            return Ok(CancelPreflightDecision::SubmitCancel);
        }
        if self.allow_cancel_by_broker_order_id_without_mapping {
            Ok(CancelPreflightDecision::SubmitCancel)
        } else {
            Err(OrderPreflightError::CancelMappingMissing)
        }
    }

    fn require_fresh_reference<'a>(
        &self,
        context: &'a OrderPreflightContext,
        now: DateTime<Utc>,
    ) -> Result<&'a OrderReferencePrice, OrderPreflightError> {
        let reference = context
            .reference_price
            .as_ref()
            .ok_or(OrderPreflightError::ReferencePriceMissing)?;
        if reference.price <= Decimal::ZERO {
            return Err(OrderPreflightError::ReferencePriceInvalid);
        }
        let age_ms = now
            .signed_duration_since(reference.received_ts)
            .num_milliseconds();
        if age_ms < 0 || age_ms as u64 > self.max_reference_age_ms {
            return Err(OrderPreflightError::ReferencePriceStale);
        }
        Ok(reference)
    }

    fn check_notional(
        &self,
        order_notional: Decimal,
        current_run_notional: Decimal,
    ) -> Result<(), OrderPreflightError> {
        if let Some(max) = self.max_notional_per_order {
            if order_notional > max {
                return Err(OrderPreflightError::OrderNotionalOutOfBounds);
            }
        }
        if let Some(max) = self.max_notional_per_run {
            if current_run_notional + order_notional > max {
                return Err(OrderPreflightError::RunNotionalOutOfBounds);
            }
        }
        Ok(())
    }

    fn check_limit_deviation(
        &self,
        limit_price: Decimal,
        reference_price: Decimal,
    ) -> Result<(), OrderPreflightError> {
        if let Some(max_bps) = self.max_limit_deviation_bps {
            let diff = if limit_price >= reference_price {
                limit_price - reference_price
            } else {
                reference_price - limit_price
            };
            if diff * Decimal::new(10_000, 0) > reference_price * Decimal::from(max_bps) {
                return Err(OrderPreflightError::LimitPriceBandExceeded);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderReferencePrice {
    pub price: Price,
    pub received_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderPreflightContext {
    pub reference_price: Option<OrderReferencePrice>,
    pub current_run_notional: Decimal,
}

impl Default for OrderPreflightContext {
    fn default() -> Self {
        Self {
            reference_price: None,
            current_run_notional: Decimal::ZERO,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CancelPreflightDecision {
    SubmitCancel,
    AlreadyTerminal,
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
    #[error("reference data is not loaded or validated")]
    ReferenceDataNotLoaded,
    #[error("reference price is missing")]
    ReferencePriceMissing,
    #[error("reference price is invalid")]
    ReferencePriceInvalid,
    #[error("reference price is stale")]
    ReferencePriceStale,
    #[error("order notional exceeds configured guard")]
    OrderNotionalOutOfBounds,
    #[error("run notional exceeds configured guard")]
    RunNotionalOutOfBounds,
    #[error("limit price is outside configured reference band")]
    LimitPriceBandExceeded,
    #[error("broker order id is missing")]
    BrokerOrderIdMissing,
    #[error("cancel mapping is missing")]
    CancelMappingMissing,
}

fn is_decimal_multiple(value: Decimal, step: Decimal) -> bool {
    step > Decimal::ZERO && value % step == Decimal::ZERO
}

fn is_terminal_order_path_state(state: OrderPathState) -> bool {
    matches!(
        state,
        OrderPathState::Terminal | OrderPathState::LocalRejected | OrderPathState::BrokerRejected
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::AccountId;
    use crate::command::{CancelOrder, PlaceOrder};
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

    fn cancel_order(request: StrategyRequestId, order_id: &str) -> CancelOrder {
        CancelOrder {
            request_id: request,
            created_ts: Utc::now(),
            ttl_ms: Some(1_000),
            account_id: account(),
            order_id: BrokerOrderId::new(order_id),
            client_order_id: None,
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
            max_notional_per_order: Some(Decimal::new(5_000, 0)),
            max_notional_per_run: Some(Decimal::new(10_000, 0)),
            max_limit_deviation_bps: None,
            max_reference_age_ms: 1_000,
            allow_cancel_by_broker_order_id_without_mapping: false,
            operator_arm: sample_arm(now),
        }
    }

    fn fresh_reference(now: DateTime<Utc>, price: Decimal) -> OrderPreflightContext {
        OrderPreflightContext {
            reference_price: Some(OrderReferencePrice {
                price,
                received_ts: now,
            }),
            current_run_notional: Decimal::ZERO,
        }
    }

    fn temp_store_path(name: &str) -> std::path::PathBuf {
        let unique = Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_default()
            .unsigned_abs();
        std::env::temp_dir().join(format!("moex_trading_order_path_{name}_{unique}.json"))
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
    fn json_file_order_path_store_persists_intent_before_submit_and_rejects_duplicates_after_restart(
    ) {
        let now = Utc::now();
        let path = temp_store_path("persist");
        let request_id = request_id(30);
        let client_order_id = ClientOrderId::new("CID000000000000030").expect("client id");
        let order = place_order(request_id, client_order_id.as_str());
        let record = OrderPathRecord::from_place_order(&order, now, None);

        {
            let mut store = JsonFileOrderPathStore::open(&path).expect("open store");
            store.insert_intent(record).expect("persist intent");
        }
        {
            let mut store = JsonFileOrderPathStore::open(&path).expect("reopen store");
            let loaded = store
                .load_by_request_id(request_id)
                .expect("record loaded after restart");
            assert_eq!(loaded.state, OrderPathState::IntentRecorded);
            assert_eq!(loaded.submit_attempt_count, 0);
            assert!(matches!(
                store.insert_intent(OrderPathRecord::from_place_order(&order, now, None)),
                Err(OrderPathStoreError::DuplicateStrategyRequestId(_))
            ));
            assert!(store.load_by_client_order_id(&client_order_id).is_some());
        }

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn json_file_order_path_store_recovers_inflight_and_preserves_submitted_after_restart() {
        let now = Utc::now();
        let inflight_path = temp_store_path("inflight");
        let submitted_path = temp_store_path("submitted");

        let inflight_order = place_order(request_id(31), "CID000000000000031");
        let mut inflight = OrderPathRecord::from_place_order(&inflight_order, now, None);
        inflight
            .transition(OrderPathEvent::BeginSubmit, now)
            .expect("begin submit");
        {
            let mut store = JsonFileOrderPathStore::open(&inflight_path).expect("open");
            store.insert_intent(inflight.clone()).expect("insert");
        }
        {
            let mut store = JsonFileOrderPathStore::open(&inflight_path).expect("reopen");
            let mut loaded = store
                .load_by_request_id(inflight.request_id)
                .expect("load inflight");
            loaded
                .recover_after_restart(now + chrono::Duration::seconds(1))
                .expect("recover inflight");
            store
                .update_record(loaded.clone())
                .expect("persist recovery");
            assert_eq!(loaded.state, OrderPathState::TimeoutUnknownPending);
        }

        let submitted_order = place_order(request_id(32), "CID000000000000032");
        let mut submitted = OrderPathRecord::from_place_order(&submitted_order, now, None);
        submitted
            .transition(OrderPathEvent::BeginSubmit, now)
            .expect("begin submit");
        submitted
            .transition(OrderPathEvent::SubmitAccepted, now)
            .expect("submitted");
        {
            let mut store = JsonFileOrderPathStore::open(&submitted_path).expect("open");
            store.insert_intent(submitted.clone()).expect("insert");
        }
        {
            let store = JsonFileOrderPathStore::open(&submitted_path).expect("reopen");
            let loaded = store
                .load_by_request_id(submitted.request_id)
                .expect("load submitted");
            assert_eq!(loaded.state, OrderPathState::Submitted);
            assert_eq!(loaded.submit_attempt_count, 1);
        }

        let _ = std::fs::remove_file(inflight_path);
        let _ = std::fs::remove_file(submitted_path);
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
    fn order_path_record_builds_synthetic_ack_without_fill_semantics() {
        let now = Utc::now();
        let order = place_order(request_id(7), "CID000000000000007");
        let mut record = OrderPathRecord::from_place_order(&order, now, None);
        record.broker_order_id = Some(BrokerOrderId::new("BROKER_TEST_1"));

        let ack = record.synthetic_ack(
            CommandAckStatus::Submitted,
            Some("synthetic submitted".to_string()),
            now,
        );

        assert_eq!(ack.request_id, record.request_id);
        assert_eq!(ack.client_order_id, Some(record.client_order_id));
        assert_eq!(
            ack.broker_order_id,
            Some(BrokerOrderId::new("BROKER_TEST_1"))
        );
        assert_eq!(ack.status, CommandAckStatus::Submitted);
        assert_eq!(ack.reason, Some("synthetic submitted".to_string()));
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
    fn preflight_validates_cancel_mapping_and_terminal_policy() {
        let now = Utc::now();
        let policy = preflight_policy(now);
        let cancel = cancel_order(request_id(40), "BROKER_TEST_40");
        let order = place_order(request_id(41), "CID000000000000041");
        let mut existing = OrderPathRecord::from_place_order(&order, now, None);
        existing.broker_order_id = Some(BrokerOrderId::new("BROKER_TEST_40"));
        existing
            .transition(OrderPathEvent::BeginSubmit, now)
            .expect("begin submit");
        existing
            .transition(OrderPathEvent::SubmitAccepted, now)
            .expect("submitted");

        assert_eq!(
            policy
                .validate_cancel_order(&cancel, now, Some(&existing))
                .expect("cancel existing"),
            CancelPreflightDecision::SubmitCancel
        );

        existing
            .transition(OrderPathEvent::MarkTerminal, now)
            .expect("terminal");
        assert_eq!(
            policy
                .validate_cancel_order(&cancel, now, Some(&existing))
                .expect("terminal cancel"),
            CancelPreflightDecision::AlreadyTerminal
        );

        assert_eq!(
            policy
                .validate_cancel_order(&cancel, now, None)
                .expect_err("missing mapping"),
            OrderPreflightError::CancelMappingMissing
        );

        let mut explicit_policy = policy.clone();
        explicit_policy.allow_cancel_by_broker_order_id_without_mapping = true;
        assert_eq!(
            explicit_policy
                .validate_cancel_order(&cancel, now, None)
                .expect("explicit broker id cancel"),
            CancelPreflightDecision::SubmitCancel
        );
    }

    #[test]
    fn preflight_rejects_cancel_without_arm_or_broker_order_id() {
        let now = Utc::now();
        let mut policy = preflight_policy(now);
        policy.operator_arm.endpoint_calls_enabled = false;
        let cancel = cancel_order(request_id(42), "BROKER_TEST_42");
        assert_eq!(
            policy
                .validate_cancel_order(&cancel, now, None)
                .expect_err("not armed"),
            OrderPreflightError::EndpointNotArmed
        );

        let policy = preflight_policy(now);
        let missing_order_id = cancel_order(request_id(43), "");
        assert_eq!(
            policy
                .validate_cancel_order(&missing_order_id, now, None)
                .expect_err("missing broker id"),
            OrderPreflightError::BrokerOrderIdMissing
        );
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

    #[test]
    fn preflight_rejects_account_symbol_arm_and_quantity_boundary_cases() {
        let now = Utc::now();
        let policy = preflight_policy(now);

        let mut wrong_account = place_order(request_id(50), "CID000000000000050");
        wrong_account.account_id = AccountId::new("ACC_TEST_OTHER");
        assert_eq!(
            policy
                .validate_place_order(&wrong_account, now)
                .expect_err("account"),
            OrderPreflightError::AccountNotAllowed
        );

        let mut missing_symbol = place_order(request_id(51), "CID000000000000051");
        missing_symbol.instrument.venue_symbol = None;
        assert_eq!(
            policy
                .validate_place_order(&missing_symbol, now)
                .expect_err("venue missing"),
            OrderPreflightError::VenueSymbolMissing
        );

        let mut wrong_symbol = place_order(request_id(52), "CID000000000000052");
        wrong_symbol.instrument.venue_symbol = Some("OTHER@TEST".to_string());
        assert_eq!(
            policy
                .validate_place_order(&wrong_symbol, now)
                .expect_err("venue allowlist"),
            OrderPreflightError::InstrumentNotAllowed
        );

        let mut zero_qty = place_order(request_id(53), "CID000000000000053");
        zero_qty.qty = Decimal::ZERO;
        assert_eq!(
            policy
                .validate_place_order(&zero_qty, now)
                .expect_err("zero"),
            OrderPreflightError::InvalidQuantity
        );

        let mut below_min = place_order(request_id(54), "CID000000000000054");
        below_min.qty = Decimal::new(5, 1);
        assert_eq!(
            policy
                .validate_place_order(&below_min, now)
                .expect_err("below min"),
            OrderPreflightError::QuantityOutOfBounds
        );

        let mut above_max = place_order(request_id(55), "CID000000000000055");
        above_max.qty = Decimal::new(4, 0);
        assert_eq!(
            policy
                .validate_place_order(&above_max, now)
                .expect_err("above max"),
            OrderPreflightError::QuantityOutOfBounds
        );

        let mut missing_audit = preflight_policy(now);
        missing_audit.operator_arm.session_id.clear();
        assert_eq!(
            missing_audit
                .validate_place_order(&place_order(request_id(56), "CID000000000000056"), now)
                .expect_err("audit"),
            OrderPreflightError::MissingArmAudit
        );
    }

    #[test]
    fn preflight_rejects_market_reference_and_notional_risk_cases() {
        let now = Utc::now();
        let policy = preflight_policy(now);

        let mut market_with_limit = place_order(request_id(60), "CID000000000000060");
        market_with_limit.order_type = OrderType::Market;
        assert_eq!(
            policy
                .validate_place_order(&market_with_limit, now)
                .expect_err("market limit"),
            OrderPreflightError::MarketOrderHasLimitPrice
        );

        let mut market = place_order(request_id(61), "CID000000000000061");
        market.order_type = OrderType::Market;
        market.limit_price = None;
        assert_eq!(
            policy
                .validate_place_order(&market, now)
                .expect_err("reference missing"),
            OrderPreflightError::ReferencePriceMissing
        );

        let stale_context = OrderPreflightContext {
            reference_price: Some(OrderReferencePrice {
                price: Decimal::new(1000, 0),
                received_ts: now - chrono::Duration::milliseconds(2_000),
            }),
            current_run_notional: Decimal::ZERO,
        };
        assert_eq!(
            policy
                .validate_place_order_with_context(&market, now, &stale_context)
                .expect_err("stale"),
            OrderPreflightError::ReferencePriceStale
        );

        let mut too_large_market = market.clone();
        too_large_market.qty = Decimal::new(2, 0);
        assert_eq!(
            policy
                .validate_place_order_with_context(
                    &too_large_market,
                    now,
                    &fresh_reference(now, Decimal::new(1000, 0))
                )
                .expect_err("market qty"),
            OrderPreflightError::MarketQuantityOutOfBounds
        );

        let mut notional_policy = policy.clone();
        notional_policy.max_notional_per_order = Some(Decimal::new(500, 0));
        assert_eq!(
            notional_policy
                .validate_place_order_with_context(
                    &market,
                    now,
                    &fresh_reference(now, Decimal::new(1000, 0))
                )
                .expect_err("notional"),
            OrderPreflightError::OrderNotionalOutOfBounds
        );

        let mut run_context = fresh_reference(now, Decimal::new(1000, 0));
        run_context.current_run_notional = Decimal::new(9_500, 0);
        assert_eq!(
            policy
                .validate_place_order_with_context(&market, now, &run_context)
                .expect_err("run notional"),
            OrderPreflightError::RunNotionalOutOfBounds
        );
    }

    #[test]
    fn preflight_rejects_limit_without_tick_missing_or_bad_price_and_reference_band() {
        let now = Utc::now();
        let policy = preflight_policy(now);

        let mut missing_price = place_order(request_id(70), "CID000000000000070");
        missing_price.limit_price = None;
        assert_eq!(
            policy
                .validate_place_order(&missing_price, now)
                .expect_err("missing limit"),
            OrderPreflightError::LimitPriceMissing
        );

        let mut zero_price = place_order(request_id(71), "CID000000000000071");
        zero_price.limit_price = Some(Decimal::ZERO);
        assert_eq!(
            policy
                .validate_place_order(&zero_price, now)
                .expect_err("zero price"),
            OrderPreflightError::InvalidLimitPrice
        );

        let mut missing_tick_policy = policy.clone();
        missing_tick_policy.price_step = None;
        assert_eq!(
            missing_tick_policy
                .validate_place_order(&place_order(request_id(72), "CID000000000000072"), now)
                .expect_err("missing tick"),
            OrderPreflightError::ReferenceDataNotLoaded
        );

        let mut band_policy = policy.clone();
        band_policy.max_limit_deviation_bps = Some(100);
        let far_price = place_order(request_id(73), "CID000000000000073");
        assert_eq!(
            band_policy
                .validate_place_order_with_context(
                    &far_price,
                    now,
                    &fresh_reference(now, Decimal::new(900, 0))
                )
                .expect_err("band"),
            OrderPreflightError::LimitPriceBandExceeded
        );
    }
}
