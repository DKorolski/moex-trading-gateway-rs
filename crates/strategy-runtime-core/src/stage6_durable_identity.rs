//! Stage 6A broker-neutral durable identity and schema types.
//!
//! This module is intentionally in-memory only. It has no journal backend,
//! transport, runtime callback, scheduler or broker-dispatch authority.
//!
//! Invalid primitive construction is unavailable to ordinary callers:
//!
//! ```compile_fail
//! use strategy_runtime_core::Stage6LifecycleSequence;
//! let _ = Stage6LifecycleSequence(0);
//! ```
//!
//! ```compile_fail
//! use strategy_runtime_core::Stage6JournalRecordId;
//! let _ = Stage6JournalRecordId("0".repeat(64));
//! ```
//!
//! ```compile_fail
//! use strategy_runtime_core::Stage6DurableCommandSnapshotV1;
//! let _ = Stage6DurableCommandSnapshotV1 { payload: todo!() };
//! ```

use broker_core::{
    BrokerAccountId, BrokerOrderId, BrokerTradeId, CancelOrder, ClientOrderId,
    HybridRuntimeAttribution, HybridRuntimeOrderRole, InstrumentId, OrderSide, OrderType,
    PlaceOrder, Price, Quantity, StrategyRequestId, TimeInForce,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const STAGE6_DURABLE_RECORD_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage6DurableIdentityError {
    PlaceClientIdentityMismatch,
    RequestIdentityMismatch,
    AccountMismatch,
    InstrumentMismatch,
    AttributionMismatch,
    ActionMismatch,
    CancelTargetMismatch,
    SequenceZero,
    InvalidSha256,
    UnsupportedSchema,
    RecordIdentityMismatch,
    PayloadDigestMismatch,
    EventPayloadMismatch,
    DecodeFailed,
    InvalidAccountIdentity,
    InvalidInstrumentIdentity,
    InvalidAttribution,
    ActionRoleMismatch,
    UnsupportedDurablePlaceOrderType,
    InvalidDurablePlacePriceShape,
    InvalidDurablePlaceQuantity,
    NonCanonicalEncoding,
    UnsupportedEventPayload,
    InvalidDispatchAttemptOrdinal,
    InvalidCancelOutcomePayload,
    InvalidActionEvent,
    InvalidConflictPayload,
}

impl std::fmt::Display for Stage6DurableIdentityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::PlaceClientIdentityMismatch => "place client identity mismatch",
            Self::RequestIdentityMismatch => "request identity mismatch",
            Self::AccountMismatch => "account mismatch",
            Self::InstrumentMismatch => "instrument mismatch",
            Self::AttributionMismatch => "attribution mismatch",
            Self::ActionMismatch => "action mismatch",
            Self::CancelTargetMismatch => "cancel target mismatch",
            Self::SequenceZero => "lifecycle sequence must be non-zero",
            Self::InvalidSha256 => "invalid canonical SHA-256 digest",
            Self::UnsupportedSchema => "unsupported journal schema",
            Self::RecordIdentityMismatch => "journal record identity mismatch",
            Self::PayloadDigestMismatch => "journal payload digest mismatch",
            Self::EventPayloadMismatch => "journal event and payload mismatch",
            Self::DecodeFailed => "journal decode failed",
            Self::InvalidAccountIdentity => "invalid durable account identity",
            Self::InvalidInstrumentIdentity => "invalid durable instrument identity",
            Self::InvalidAttribution => "invalid durable hybrid attribution",
            Self::ActionRoleMismatch => "durable action and attribution role mismatch",
            Self::UnsupportedDurablePlaceOrderType => "unsupported durable Place order type",
            Self::InvalidDurablePlacePriceShape => "invalid durable Place price shape",
            Self::InvalidDurablePlaceQuantity => "durable Place quantity must be positive",
            Self::NonCanonicalEncoding => "journal bytes are not canonical",
            Self::UnsupportedEventPayload => "event payload is reserved for a later stage",
            Self::InvalidDispatchAttemptOrdinal => "dispatch attempt ordinal must be non-zero",
            Self::InvalidCancelOutcomePayload => "invalid cancel outcome payload",
            Self::InvalidActionEvent => "journal event is invalid for durable action",
            Self::InvalidConflictPayload => "invalid conflict audit payload",
        })
    }
}

impl std::error::Error for Stage6DurableIdentityError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage6DurableActionKind {
    Place,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage6DurableRequestIdentityV1 {
    strategy_request_id: StrategyRequestId,
    durable_client_order_id: ClientOrderId,
    account_id: BrokerAccountId,
    instrument: InstrumentId,
    attribution: HybridRuntimeAttribution,
    action: Stage6DurableActionKind,
    target_broker_order_id: Option<BrokerOrderId>,
    target_order_client_order_id: Option<ClientOrderId>,
}

impl Stage6DurableRequestIdentityV1 {
    pub fn from_place(
        command: &PlaceOrder,
        attribution: HybridRuntimeAttribution,
    ) -> Result<Self, Stage6DurableIdentityError> {
        let durable = ClientOrderId::from_strategy_request(command.request_id);
        if command.client_order_id != durable {
            return Err(Stage6DurableIdentityError::PlaceClientIdentityMismatch);
        }
        if command.comment.as_deref() != Some(attribution.internal_comment()) {
            return Err(Stage6DurableIdentityError::AttributionMismatch);
        }
        let value = Self {
            strategy_request_id: command.request_id,
            durable_client_order_id: durable,
            account_id: command.account_id.clone(),
            instrument: command.instrument.clone(),
            attribution,
            action: Stage6DurableActionKind::Place,
            target_broker_order_id: None,
            target_order_client_order_id: None,
        };
        value.validate_self()?;
        Ok(value)
    }

    pub fn from_cancel(
        command: &CancelOrder,
        instrument: InstrumentId,
        attribution: HybridRuntimeAttribution,
    ) -> Result<Self, Stage6DurableIdentityError> {
        let value = Self {
            strategy_request_id: command.request_id,
            durable_client_order_id: ClientOrderId::from_strategy_request(command.request_id),
            account_id: command.account_id.clone(),
            instrument,
            attribution,
            action: Stage6DurableActionKind::Cancel,
            target_broker_order_id: Some(command.order_id.clone()),
            target_order_client_order_id: command.client_order_id.clone(),
        };
        value.validate_self()?;
        Ok(value)
    }

    pub fn strategy_request_id(&self) -> StrategyRequestId {
        self.strategy_request_id
    }
    pub fn durable_client_order_id(&self) -> &ClientOrderId {
        &self.durable_client_order_id
    }
    pub fn target_order_client_order_id(&self) -> Option<&ClientOrderId> {
        self.target_order_client_order_id.as_ref()
    }
    pub fn target_broker_order_id(&self) -> Option<&BrokerOrderId> {
        self.target_broker_order_id.as_ref()
    }
    pub fn account_id(&self) -> &BrokerAccountId {
        &self.account_id
    }
    pub fn instrument(&self) -> &InstrumentId {
        &self.instrument
    }
    pub fn attribution(&self) -> &HybridRuntimeAttribution {
        &self.attribution
    }
    pub fn action(&self) -> Stage6DurableActionKind {
        self.action
    }

    pub(crate) fn validate_self(&self) -> Result<(), Stage6DurableIdentityError> {
        if self.durable_client_order_id
            != ClientOrderId::from_strategy_request(self.strategy_request_id)
        {
            return Err(Stage6DurableIdentityError::PlaceClientIdentityMismatch);
        }
        validate_durable_domain(
            &self.account_id,
            &self.instrument,
            &self.attribution,
            self.action,
        )?;
        match self.action {
            Stage6DurableActionKind::Place
                if self.target_broker_order_id.is_none()
                    && self.target_order_client_order_id.is_none() =>
            {
                Ok(())
            }
            Stage6DurableActionKind::Cancel
                if self.target_broker_order_id.is_some()
                    && self.attribution.role()
                        == Some(broker_core::HybridRuntimeOrderRole::Cancel) =>
            {
                Ok(())
            }
            Stage6DurableActionKind::Place => Err(Stage6DurableIdentityError::ActionMismatch),
            Stage6DurableActionKind::Cancel => {
                Err(Stage6DurableIdentityError::CancelTargetMismatch)
            }
        }
    }
}

fn validate_durable_domain(
    account_id: &BrokerAccountId,
    instrument: &InstrumentId,
    attribution: &HybridRuntimeAttribution,
    action: Stage6DurableActionKind,
) -> Result<(), Stage6DurableIdentityError> {
    if account_id.as_str().is_empty() {
        return Err(Stage6DurableIdentityError::InvalidAccountIdentity);
    }
    if instrument.symbol.is_empty()
        || instrument
            .venue_symbol
            .as_deref()
            .is_some_and(str::is_empty)
    {
        return Err(Stage6DurableIdentityError::InvalidInstrumentIdentity);
    }
    if attribution.strategy_id().is_empty()
        || attribution.cycle_id().is_empty()
        || attribution.owner().is_none()
        || attribution.role().is_none()
        || attribution.validate_source_equivalence().is_err()
    {
        return Err(Stage6DurableIdentityError::InvalidAttribution);
    }
    let compatible = matches!(
        (action, attribution.role()),
        (
            Stage6DurableActionKind::Place,
            Some(
                HybridRuntimeOrderRole::Entry
                    | HybridRuntimeOrderRole::Exit
                    | HybridRuntimeOrderRole::TakeProfit
                    | HybridRuntimeOrderRole::StopLoss,
            ),
        ) | (
            Stage6DurableActionKind::Cancel,
            Some(HybridRuntimeOrderRole::Cancel)
        )
    );
    if !compatible {
        return Err(Stage6DurableIdentityError::ActionRoleMismatch);
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage6DurableRequestIdentityWireV1 {
    strategy_request_id: StrategyRequestId,
    durable_client_order_id: ClientOrderId,
    account_id: BrokerAccountId,
    instrument: InstrumentId,
    attribution: HybridRuntimeAttribution,
    action: Stage6DurableActionKind,
    target_broker_order_id: Option<BrokerOrderId>,
    target_order_client_order_id: Option<ClientOrderId>,
}

impl<'de> Deserialize<'de> for Stage6DurableRequestIdentityV1 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = Stage6DurableRequestIdentityWireV1::deserialize(deserializer)?;
        let value = Self {
            strategy_request_id: wire.strategy_request_id,
            durable_client_order_id: wire.durable_client_order_id,
            account_id: wire.account_id,
            instrument: wire.instrument,
            attribution: wire.attribution,
            action: wire.action,
            target_broker_order_id: wire.target_broker_order_id,
            target_order_client_order_id: wire.target_order_client_order_id,
        };
        value.validate_self().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Stage6DurableCommandSnapshotV1 {
    payload: Stage6DurableCommandPayloadV1,
}

/// Broker-neutral immutable PLACE shape used to prove the original target of
/// a later CANCEL. It carries no send or journal authority.
#[derive(Clone, PartialEq, Serialize)]
pub struct Stage6DurablePlaceOrderShapeV1 {
    side: OrderSide,
    order_type: OrderType,
    quantity: Quantity,
    limit_price: Option<Price>,
    time_in_force: TimeInForce,
}

impl Stage6DurablePlaceOrderShapeV1 {
    pub fn new(
        side: OrderSide,
        order_type: OrderType,
        quantity: Quantity,
        limit_price: Option<Price>,
        time_in_force: TimeInForce,
    ) -> Result<Self, Stage6DurableIdentityError> {
        validate_durable_place_shape(order_type, &quantity, &limit_price)?;
        Ok(Self {
            side,
            order_type,
            quantity,
            limit_price,
            time_in_force,
        })
    }

    pub(crate) fn matches(
        &self,
        side: OrderSide,
        order_type: OrderType,
        quantity: Quantity,
        limit_price: Option<Price>,
        time_in_force: TimeInForce,
    ) -> bool {
        self.side == side
            && self.order_type == order_type
            && self.quantity == quantity
            && self.limit_price == limit_price
            && self.time_in_force == time_in_force
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command_kind", rename_all = "snake_case", deny_unknown_fields)]
enum Stage6DurableCommandPayloadV1 {
    Place {
        request_id: StrategyRequestId,
        durable_client_order_id: ClientOrderId,
        account_id: BrokerAccountId,
        instrument: InstrumentId,
        side: OrderSide,
        order_type: OrderType,
        quantity: Quantity,
        limit_price: Option<Price>,
        time_in_force: TimeInForce,
        ttl_ms: Option<u64>,
        created_ts: DateTime<Utc>,
        attribution: HybridRuntimeAttribution,
    },
    Cancel {
        request_id: StrategyRequestId,
        durable_cancel_client_order_id: ClientOrderId,
        account_id: BrokerAccountId,
        instrument: InstrumentId,
        target_broker_order_id: BrokerOrderId,
        target_order_client_order_id: Option<ClientOrderId>,
        ttl_ms: Option<u64>,
        created_ts: DateTime<Utc>,
        attribution: HybridRuntimeAttribution,
    },
}

impl Stage6DurableCommandSnapshotV1 {
    pub fn from_place(
        identity: &Stage6DurableRequestIdentityV1,
        command: &PlaceOrder,
    ) -> Result<Self, Stage6DurableIdentityError> {
        identity.validate_self()?;
        validate_common(
            identity,
            command.request_id,
            &command.account_id,
            &command.instrument,
            Stage6DurableActionKind::Place,
        )?;
        if command.client_order_id != *identity.durable_client_order_id() {
            return Err(Stage6DurableIdentityError::PlaceClientIdentityMismatch);
        }
        if command.comment.as_deref() != Some(identity.attribution().internal_comment()) {
            return Err(Stage6DurableIdentityError::AttributionMismatch);
        }
        let value = Self {
            payload: Stage6DurableCommandPayloadV1::Place {
                request_id: command.request_id,
                durable_client_order_id: command.client_order_id.clone(),
                account_id: command.account_id.clone(),
                instrument: command.instrument.clone(),
                side: command.side,
                order_type: command.order_type,
                quantity: command.qty,
                limit_price: command.limit_price,
                time_in_force: command.time_in_force,
                ttl_ms: command.ttl_ms,
                created_ts: command.created_ts,
                attribution: identity.attribution().clone(),
            },
        };
        value.validate_intrinsic()?;
        Ok(value)
    }

    pub fn from_cancel(
        identity: &Stage6DurableRequestIdentityV1,
        command: &CancelOrder,
    ) -> Result<Self, Stage6DurableIdentityError> {
        identity.validate_self()?;
        if identity.strategy_request_id != command.request_id {
            return Err(Stage6DurableIdentityError::RequestIdentityMismatch);
        }
        if identity.account_id != command.account_id {
            return Err(Stage6DurableIdentityError::AccountMismatch);
        }
        if identity.action != Stage6DurableActionKind::Cancel {
            return Err(Stage6DurableIdentityError::ActionMismatch);
        }
        if identity.target_broker_order_id.as_ref() != Some(&command.order_id)
            || identity.target_order_client_order_id != command.client_order_id
        {
            return Err(Stage6DurableIdentityError::CancelTargetMismatch);
        }
        let value = Self {
            payload: Stage6DurableCommandPayloadV1::Cancel {
                request_id: command.request_id,
                durable_cancel_client_order_id: identity.durable_client_order_id.clone(),
                account_id: command.account_id.clone(),
                instrument: identity.instrument.clone(),
                target_broker_order_id: command.order_id.clone(),
                target_order_client_order_id: command.client_order_id.clone(),
                ttl_ms: command.ttl_ms,
                created_ts: command.created_ts,
                attribution: identity.attribution.clone(),
            },
        };
        value.validate_intrinsic()?;
        Ok(value)
    }

    pub fn action(&self) -> Stage6DurableActionKind {
        match self.payload {
            Stage6DurableCommandPayloadV1::Place { .. } => Stage6DurableActionKind::Place,
            Stage6DurableCommandPayloadV1::Cancel { .. } => Stage6DurableActionKind::Cancel,
        }
    }

    pub fn place_order_shape(&self) -> Option<Stage6DurablePlaceOrderShapeV1> {
        match &self.payload {
            Stage6DurableCommandPayloadV1::Place {
                side,
                order_type,
                quantity,
                limit_price,
                time_in_force,
                ..
            } => Some(Stage6DurablePlaceOrderShapeV1 {
                side: *side,
                order_type: *order_type,
                quantity: *quantity,
                limit_price: *limit_price,
                time_in_force: *time_in_force,
            }),
            Stage6DurableCommandPayloadV1::Cancel { .. } => None,
        }
    }

    fn validate_intrinsic(&self) -> Result<(), Stage6DurableIdentityError> {
        match &self.payload {
            Stage6DurableCommandPayloadV1::Place {
                request_id,
                durable_client_order_id,
                account_id,
                instrument,
                order_type,
                quantity,
                limit_price,
                attribution,
                ..
            } => {
                validate_durable_domain(
                    account_id,
                    instrument,
                    attribution,
                    Stage6DurableActionKind::Place,
                )?;
                if durable_client_order_id != &ClientOrderId::from_strategy_request(*request_id) {
                    return Err(Stage6DurableIdentityError::PlaceClientIdentityMismatch);
                }
                validate_durable_place_shape(*order_type, quantity, limit_price)?;
            }
            Stage6DurableCommandPayloadV1::Cancel {
                request_id,
                durable_cancel_client_order_id,
                account_id,
                instrument,
                attribution,
                ..
            } => {
                validate_durable_domain(
                    account_id,
                    instrument,
                    attribution,
                    Stage6DurableActionKind::Cancel,
                )?;
                if durable_cancel_client_order_id
                    != &ClientOrderId::from_strategy_request(*request_id)
                {
                    return Err(Stage6DurableIdentityError::CancelTargetMismatch);
                }
            }
        }
        Ok(())
    }
}

fn validate_durable_place_shape(
    order_type: OrderType,
    quantity: &Quantity,
    limit_price: &Option<Price>,
) -> Result<(), Stage6DurableIdentityError> {
    if quantity <= &Quantity::ZERO {
        return Err(Stage6DurableIdentityError::InvalidDurablePlaceQuantity);
    }
    match order_type {
        OrderType::Market if limit_price.is_none() => Ok(()),
        OrderType::Market => Err(Stage6DurableIdentityError::InvalidDurablePlacePriceShape),
        OrderType::Limit => match limit_price {
            Some(price) if price > &Price::ZERO => Ok(()),
            _ => Err(Stage6DurableIdentityError::InvalidDurablePlacePriceShape),
        },
        OrderType::Stop
        | OrderType::StopLimit
        | OrderType::TakeProfit
        | OrderType::TakeProfitLimit => {
            Err(Stage6DurableIdentityError::UnsupportedDurablePlaceOrderType)
        }
    }
}

impl<'de> Deserialize<'de> for Stage6DurableCommandSnapshotV1 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Self {
            payload: Stage6DurableCommandPayloadV1::deserialize(deserializer)?,
        };
        value
            .validate_intrinsic()
            .map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}

fn validate_common(
    identity: &Stage6DurableRequestIdentityV1,
    request: StrategyRequestId,
    account: &BrokerAccountId,
    instrument: &InstrumentId,
    action: Stage6DurableActionKind,
) -> Result<(), Stage6DurableIdentityError> {
    if identity.strategy_request_id != request {
        return Err(Stage6DurableIdentityError::RequestIdentityMismatch);
    }
    if &identity.account_id != account {
        return Err(Stage6DurableIdentityError::AccountMismatch);
    }
    if &identity.instrument != instrument {
        return Err(Stage6DurableIdentityError::InstrumentMismatch);
    }
    if identity.action != action {
        return Err(Stage6DurableIdentityError::ActionMismatch);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Stage6LifecycleSequence(u64);

impl Stage6LifecycleSequence {
    pub fn new(value: u64) -> Result<Self, Stage6DurableIdentityError> {
        if value == 0 {
            Err(Stage6DurableIdentityError::SequenceZero)
        } else {
            Ok(Self(value))
        }
    }
    pub fn get(self) -> u64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for Stage6LifecycleSequence {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = u64::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct Stage6Sha256Digest(String);

impl Stage6Sha256Digest {
    pub fn parse(value: impl Into<String>) -> Result<Self, Stage6DurableIdentityError> {
        let value = value.into();
        if value.len() != 64
            || value == "0".repeat(64)
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(Stage6DurableIdentityError::InvalidSha256);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub(crate) fn of(bytes: &[u8]) -> Self {
        Self(hex_sha256(bytes))
    }
}

impl<'de> Deserialize<'de> for Stage6Sha256Digest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct Stage6JournalRecordId(String);

impl Stage6JournalRecordId {
    pub(crate) fn derive(request: StrategyRequestId, sequence: Stage6LifecycleSequence) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"stage6-journal-record-v1");
        hasher.update(request.as_uuid().as_bytes());
        hasher.update(sequence.get().to_be_bytes());
        Self(
            hasher
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        )
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Stage6JournalRecordId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value.len() == 64
            && value != "0".repeat(64)
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            Ok(Self(value))
        } else {
            Err(serde::de::Error::custom("invalid journal record id"))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage6JournalEventKind {
    RequestAccepted,
    DispatchAttemptRecorded,
    BrokerOrderObserved,
    BrokerTradeObserved,
    CancelOutcomeObserved,
    ReconciliationObserved,
    RequestFinalized,
    ConflictObserved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "disposition", rename_all = "snake_case", deny_unknown_fields)]
pub enum Stage6ReconciliationDispositionV1 {
    NoBrokerOrderFound,
    BrokerOrderFound { broker_order_id: BrokerOrderId },
    Inconclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage6CancelOutcomeV1 {
    Canceled,
    AlreadyTerminalNonExecution,
    ExecutionObserved,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage6RequestFinalDispositionV1 {
    Completed,
    Canceled,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage6ConflictKindV1 {
    ConflictingReplay,
    IdentityDrift,
    BrokerOrderConflict,
    BrokerTradeConflict,
    CausalLinkConflict,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "payload_kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Stage6JournalPayloadV1 {
    RequestAccepted {
        command: Box<Stage6DurableCommandSnapshotV1>,
    },
    BrokerOrderObserved {
        broker_order_id: BrokerOrderId,
    },
    BrokerTradeObserved {
        broker_trade_id: BrokerTradeId,
        broker_order_id: BrokerOrderId,
    },
    DispatchAttemptRecorded {
        attempt_ordinal: u32,
        accepted_request_payload_sha256: Stage6Sha256Digest,
    },
    CancelOutcomeObserved {
        target_broker_order_id: BrokerOrderId,
        outcome: Stage6CancelOutcomeV1,
    },
    ReconciliationObserved {
        disposition: Stage6ReconciliationDispositionV1,
    },
    RequestFinalized {
        disposition: Stage6RequestFinalDispositionV1,
    },
    ConflictObserved {
        conflict_kind: Stage6ConflictKindV1,
        conflicting_journal_record_id: Option<Stage6JournalRecordId>,
        expected_digest: Option<Stage6Sha256Digest>,
        observed_digest: Option<Stage6Sha256Digest>,
    },
    Marker,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stage6JournalRecordV1 {
    schema_version: u16,
    journal_record_id: Stage6JournalRecordId,
    lifecycle_sequence: Stage6LifecycleSequence,
    previous_record_id: Option<Stage6JournalRecordId>,
    causal_parent_id: Option<Stage6JournalRecordId>,
    durable_request_identity: Stage6DurableRequestIdentityV1,
    event_kind: Stage6JournalEventKind,
    payload: Stage6JournalPayloadV1,
    canonical_payload_sha256: Stage6Sha256Digest,
    source_evidence_sha256: Stage6Sha256Digest,
}

impl Stage6JournalRecordV1 {
    pub fn request_accepted(
        identity: Stage6DurableRequestIdentityV1,
        command: Stage6DurableCommandSnapshotV1,
        sequence: Stage6LifecycleSequence,
        previous: Option<Stage6JournalRecordId>,
        causal_parent: Option<Stage6JournalRecordId>,
        source_evidence: Stage6Sha256Digest,
    ) -> Result<Self, Stage6DurableIdentityError> {
        validate_snapshot_identity(&identity, &command)?;
        Self::build(
            identity,
            Stage6JournalEventKind::RequestAccepted,
            Stage6JournalPayloadV1::RequestAccepted {
                command: Box::new(command),
            },
            sequence,
            previous,
            causal_parent,
            source_evidence,
        )
    }

    pub fn dispatch_attempt_recorded(
        identity: Stage6DurableRequestIdentityV1,
        attempt_ordinal: u32,
        accepted_request_payload_sha256: Stage6Sha256Digest,
        sequence: Stage6LifecycleSequence,
        previous: Option<Stage6JournalRecordId>,
        source_evidence: Stage6Sha256Digest,
    ) -> Result<Self, Stage6DurableIdentityError> {
        if attempt_ordinal == 0 {
            return Err(Stage6DurableIdentityError::InvalidDispatchAttemptOrdinal);
        }
        Self::build(
            identity,
            Stage6JournalEventKind::DispatchAttemptRecorded,
            Stage6JournalPayloadV1::DispatchAttemptRecorded {
                attempt_ordinal,
                accepted_request_payload_sha256,
            },
            sequence,
            previous.clone(),
            previous,
            source_evidence,
        )
    }

    pub fn broker_order_observed(
        identity: Stage6DurableRequestIdentityV1,
        broker_order_id: BrokerOrderId,
        sequence: Stage6LifecycleSequence,
        previous: Option<Stage6JournalRecordId>,
        source_evidence: Stage6Sha256Digest,
    ) -> Result<Self, Stage6DurableIdentityError> {
        if identity.action() != Stage6DurableActionKind::Place {
            return Err(Stage6DurableIdentityError::InvalidActionEvent);
        }
        Self::build(
            identity,
            Stage6JournalEventKind::BrokerOrderObserved,
            Stage6JournalPayloadV1::BrokerOrderObserved { broker_order_id },
            sequence,
            previous.clone(),
            previous,
            source_evidence,
        )
    }

    pub fn broker_trade_observed(
        identity: Stage6DurableRequestIdentityV1,
        broker_trade_id: BrokerTradeId,
        broker_order_id: BrokerOrderId,
        sequence: Stage6LifecycleSequence,
        previous: Option<Stage6JournalRecordId>,
        source_evidence: Stage6Sha256Digest,
    ) -> Result<Self, Stage6DurableIdentityError> {
        if identity.action() != Stage6DurableActionKind::Place {
            return Err(Stage6DurableIdentityError::InvalidActionEvent);
        }
        Self::build(
            identity,
            Stage6JournalEventKind::BrokerTradeObserved,
            Stage6JournalPayloadV1::BrokerTradeObserved {
                broker_trade_id,
                broker_order_id,
            },
            sequence,
            previous.clone(),
            previous,
            source_evidence,
        )
    }

    pub fn cancel_outcome_observed(
        identity: Stage6DurableRequestIdentityV1,
        target_broker_order_id: BrokerOrderId,
        outcome: Stage6CancelOutcomeV1,
        sequence: Stage6LifecycleSequence,
        previous: Option<Stage6JournalRecordId>,
        source_evidence: Stage6Sha256Digest,
    ) -> Result<Self, Stage6DurableIdentityError> {
        if identity.action() != Stage6DurableActionKind::Cancel
            || identity.target_broker_order_id() != Some(&target_broker_order_id)
        {
            return Err(Stage6DurableIdentityError::InvalidCancelOutcomePayload);
        }
        Self::build(
            identity,
            Stage6JournalEventKind::CancelOutcomeObserved,
            Stage6JournalPayloadV1::CancelOutcomeObserved {
                target_broker_order_id,
                outcome,
            },
            sequence,
            previous.clone(),
            previous,
            source_evidence,
        )
    }

    pub fn reconciliation_observed(
        identity: Stage6DurableRequestIdentityV1,
        disposition: Stage6ReconciliationDispositionV1,
        sequence: Stage6LifecycleSequence,
        previous: Option<Stage6JournalRecordId>,
        source_evidence: Stage6Sha256Digest,
    ) -> Result<Self, Stage6DurableIdentityError> {
        validate_action_event(
            identity.action(),
            &Stage6JournalPayloadV1::ReconciliationObserved {
                disposition: disposition.clone(),
            },
        )?;
        Self::build(
            identity,
            Stage6JournalEventKind::ReconciliationObserved,
            Stage6JournalPayloadV1::ReconciliationObserved { disposition },
            sequence,
            previous.clone(),
            previous,
            source_evidence,
        )
    }

    pub fn request_finalized(
        identity: Stage6DurableRequestIdentityV1,
        disposition: Stage6RequestFinalDispositionV1,
        sequence: Stage6LifecycleSequence,
        previous: Option<Stage6JournalRecordId>,
        source_evidence: Stage6Sha256Digest,
    ) -> Result<Self, Stage6DurableIdentityError> {
        Self::build(
            identity,
            Stage6JournalEventKind::RequestFinalized,
            Stage6JournalPayloadV1::RequestFinalized { disposition },
            sequence,
            previous.clone(),
            previous,
            source_evidence,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn conflict_observed(
        identity: Stage6DurableRequestIdentityV1,
        conflict_kind: Stage6ConflictKindV1,
        conflicting_journal_record_id: Option<Stage6JournalRecordId>,
        expected_digest: Option<Stage6Sha256Digest>,
        observed_digest: Option<Stage6Sha256Digest>,
        sequence: Stage6LifecycleSequence,
        previous: Option<Stage6JournalRecordId>,
        source_evidence: Stage6Sha256Digest,
    ) -> Result<Self, Stage6DurableIdentityError> {
        if conflicting_journal_record_id.is_none()
            && expected_digest.is_none()
            && observed_digest.is_none()
            || expected_digest.is_some() != observed_digest.is_some()
        {
            return Err(Stage6DurableIdentityError::InvalidConflictPayload);
        }
        Self::build(
            identity,
            Stage6JournalEventKind::ConflictObserved,
            Stage6JournalPayloadV1::ConflictObserved {
                conflict_kind,
                conflicting_journal_record_id,
                expected_digest,
                observed_digest,
            },
            sequence,
            previous.clone(),
            previous,
            source_evidence,
        )
    }

    fn build(
        identity: Stage6DurableRequestIdentityV1,
        event_kind: Stage6JournalEventKind,
        payload: Stage6JournalPayloadV1,
        sequence: Stage6LifecycleSequence,
        previous: Option<Stage6JournalRecordId>,
        causal_parent: Option<Stage6JournalRecordId>,
        source_evidence: Stage6Sha256Digest,
    ) -> Result<Self, Stage6DurableIdentityError> {
        identity.validate_self()?;
        let payload_bytes =
            serde_json::to_vec(&payload).expect("fixed Stage 6A payload serializes");
        let value = Self {
            schema_version: STAGE6_DURABLE_RECORD_SCHEMA_VERSION,
            journal_record_id: Stage6JournalRecordId::derive(
                identity.strategy_request_id,
                sequence,
            ),
            lifecycle_sequence: sequence,
            previous_record_id: previous,
            causal_parent_id: causal_parent,
            durable_request_identity: identity,
            event_kind,
            payload,
            canonical_payload_sha256: Stage6Sha256Digest::of(&payload_bytes),
            source_evidence_sha256: source_evidence,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn encode_canonical(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("Stage 6A record serializes")
    }
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Stage6DurableIdentityError> {
        let record: Self =
            serde_json::from_slice(bytes).map_err(|_| Stage6DurableIdentityError::DecodeFailed)?;
        record.validate()?;
        if record.encode_canonical() != bytes {
            return Err(Stage6DurableIdentityError::NonCanonicalEncoding);
        }
        Ok(record)
    }
    fn validate(&self) -> Result<(), Stage6DurableIdentityError> {
        if self.schema_version != STAGE6_DURABLE_RECORD_SCHEMA_VERSION {
            return Err(Stage6DurableIdentityError::UnsupportedSchema);
        }
        if self.journal_record_id
            != Stage6JournalRecordId::derive(
                self.durable_request_identity.strategy_request_id,
                self.lifecycle_sequence,
            )
        {
            return Err(Stage6DurableIdentityError::RecordIdentityMismatch);
        }
        let digest = Stage6Sha256Digest::of(
            &serde_json::to_vec(&self.payload)
                .map_err(|_| Stage6DurableIdentityError::DecodeFailed)?,
        );
        if self.canonical_payload_sha256 != digest {
            return Err(Stage6DurableIdentityError::PayloadDigestMismatch);
        }
        if matches!(self.payload, Stage6JournalPayloadV1::Marker) {
            return Err(Stage6DurableIdentityError::UnsupportedEventPayload);
        }
        if !event_matches_payload(self.event_kind, &self.payload) {
            return Err(Stage6DurableIdentityError::EventPayloadMismatch);
        }
        validate_action_event(self.durable_request_identity.action(), &self.payload)?;
        if let Stage6JournalPayloadV1::RequestAccepted { command } = &self.payload {
            validate_snapshot_identity(&self.durable_request_identity, command)?;
        }
        match &self.payload {
            Stage6JournalPayloadV1::DispatchAttemptRecorded {
                attempt_ordinal, ..
            } if *attempt_ordinal == 0 => {
                return Err(Stage6DurableIdentityError::InvalidDispatchAttemptOrdinal);
            }
            Stage6JournalPayloadV1::CancelOutcomeObserved {
                target_broker_order_id,
                ..
            } if self.durable_request_identity.action() != Stage6DurableActionKind::Cancel
                || self.durable_request_identity.target_broker_order_id()
                    != Some(target_broker_order_id) =>
            {
                return Err(Stage6DurableIdentityError::InvalidCancelOutcomePayload);
            }
            Stage6JournalPayloadV1::ConflictObserved {
                conflicting_journal_record_id,
                expected_digest,
                observed_digest,
                ..
            } if conflicting_journal_record_id.is_none()
                && expected_digest.is_none()
                && observed_digest.is_none()
                || expected_digest.is_some() != observed_digest.is_some() =>
            {
                return Err(Stage6DurableIdentityError::InvalidConflictPayload);
            }
            _ => {}
        }
        self.durable_request_identity.validate_self()?;
        Ok(())
    }
    pub fn journal_record_id(&self) -> &Stage6JournalRecordId {
        &self.journal_record_id
    }
    pub fn canonical_payload_sha256(&self) -> &Stage6Sha256Digest {
        &self.canonical_payload_sha256
    }
    pub fn source_evidence_sha256(&self) -> &Stage6Sha256Digest {
        &self.source_evidence_sha256
    }
    pub fn lifecycle_sequence(&self) -> Stage6LifecycleSequence {
        self.lifecycle_sequence
    }
    pub(crate) fn durable_request_identity(&self) -> &Stage6DurableRequestIdentityV1 {
        &self.durable_request_identity
    }
    pub(crate) fn event_kind(&self) -> Stage6JournalEventKind {
        self.event_kind
    }
    pub(crate) fn previous_record_id(&self) -> Option<&Stage6JournalRecordId> {
        self.previous_record_id.as_ref()
    }
    pub(crate) fn causal_parent_id(&self) -> Option<&Stage6JournalRecordId> {
        self.causal_parent_id.as_ref()
    }
    pub(crate) fn payload(&self) -> &Stage6JournalPayloadV1 {
        &self.payload
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage6JournalRecordWireV1 {
    schema_version: u16,
    journal_record_id: Stage6JournalRecordId,
    lifecycle_sequence: Stage6LifecycleSequence,
    previous_record_id: Option<Stage6JournalRecordId>,
    causal_parent_id: Option<Stage6JournalRecordId>,
    durable_request_identity: Stage6DurableRequestIdentityV1,
    event_kind: Stage6JournalEventKind,
    payload: Stage6JournalPayloadV1,
    canonical_payload_sha256: Stage6Sha256Digest,
    source_evidence_sha256: Stage6Sha256Digest,
}

impl<'de> Deserialize<'de> for Stage6JournalRecordV1 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = Stage6JournalRecordWireV1::deserialize(deserializer)?;
        let value = Self {
            schema_version: wire.schema_version,
            journal_record_id: wire.journal_record_id,
            lifecycle_sequence: wire.lifecycle_sequence,
            previous_record_id: wire.previous_record_id,
            causal_parent_id: wire.causal_parent_id,
            durable_request_identity: wire.durable_request_identity,
            event_kind: wire.event_kind,
            payload: wire.payload,
            canonical_payload_sha256: wire.canonical_payload_sha256,
            source_evidence_sha256: wire.source_evidence_sha256,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}

fn validate_snapshot_identity(
    identity: &Stage6DurableRequestIdentityV1,
    command: &Stage6DurableCommandSnapshotV1,
) -> Result<(), Stage6DurableIdentityError> {
    match &command.payload {
        Stage6DurableCommandPayloadV1::Place {
            request_id,
            durable_client_order_id,
            account_id,
            instrument,
            attribution,
            ..
        } => {
            validate_common(
                identity,
                *request_id,
                account_id,
                instrument,
                Stage6DurableActionKind::Place,
            )?;
            if durable_client_order_id != identity.durable_client_order_id() {
                return Err(Stage6DurableIdentityError::PlaceClientIdentityMismatch);
            }
            if attribution != identity.attribution() {
                return Err(Stage6DurableIdentityError::AttributionMismatch);
            }
        }
        Stage6DurableCommandPayloadV1::Cancel {
            request_id,
            durable_cancel_client_order_id,
            account_id,
            instrument,
            target_broker_order_id,
            target_order_client_order_id,
            attribution,
            ..
        } => {
            validate_common(
                identity,
                *request_id,
                account_id,
                instrument,
                Stage6DurableActionKind::Cancel,
            )?;
            if durable_cancel_client_order_id != identity.durable_client_order_id()
                || Some(target_broker_order_id) != identity.target_broker_order_id()
                || target_order_client_order_id.as_ref() != identity.target_order_client_order_id()
            {
                return Err(Stage6DurableIdentityError::CancelTargetMismatch);
            }
            if attribution != identity.attribution() {
                return Err(Stage6DurableIdentityError::AttributionMismatch);
            }
        }
    }
    Ok(())
}

fn event_matches_payload(kind: Stage6JournalEventKind, payload: &Stage6JournalPayloadV1) -> bool {
    matches!(
        (kind, payload),
        (
            Stage6JournalEventKind::RequestAccepted,
            Stage6JournalPayloadV1::RequestAccepted { .. }
        ) | (
            Stage6JournalEventKind::BrokerOrderObserved,
            Stage6JournalPayloadV1::BrokerOrderObserved { .. }
        ) | (
            Stage6JournalEventKind::BrokerTradeObserved,
            Stage6JournalPayloadV1::BrokerTradeObserved { .. }
        ) | (
            Stage6JournalEventKind::DispatchAttemptRecorded,
            Stage6JournalPayloadV1::DispatchAttemptRecorded { .. }
        ) | (
            Stage6JournalEventKind::CancelOutcomeObserved,
            Stage6JournalPayloadV1::CancelOutcomeObserved { .. }
        ) | (
            Stage6JournalEventKind::ReconciliationObserved,
            Stage6JournalPayloadV1::ReconciliationObserved { .. }
        ) | (
            Stage6JournalEventKind::RequestFinalized,
            Stage6JournalPayloadV1::RequestFinalized { .. }
        ) | (
            Stage6JournalEventKind::ConflictObserved,
            Stage6JournalPayloadV1::ConflictObserved { .. }
        )
    )
}

fn validate_action_event(
    action: Stage6DurableActionKind,
    payload: &Stage6JournalPayloadV1,
) -> Result<(), Stage6DurableIdentityError> {
    let allowed = matches!(
        (action, payload),
        (
            Stage6DurableActionKind::Place,
            Stage6JournalPayloadV1::RequestAccepted { .. }
                | Stage6JournalPayloadV1::DispatchAttemptRecorded { .. }
                | Stage6JournalPayloadV1::BrokerOrderObserved { .. }
                | Stage6JournalPayloadV1::BrokerTradeObserved { .. }
                | Stage6JournalPayloadV1::ReconciliationObserved { .. }
                | Stage6JournalPayloadV1::RequestFinalized { .. }
                | Stage6JournalPayloadV1::ConflictObserved { .. },
        ) | (
            Stage6DurableActionKind::Cancel,
            Stage6JournalPayloadV1::RequestAccepted { .. }
                | Stage6JournalPayloadV1::DispatchAttemptRecorded { .. }
                | Stage6JournalPayloadV1::CancelOutcomeObserved { .. }
                | Stage6JournalPayloadV1::ReconciliationObserved {
                    disposition: Stage6ReconciliationDispositionV1::Inconclusive,
                }
                | Stage6JournalPayloadV1::RequestFinalized { .. }
                | Stage6JournalPayloadV1::ConflictObserved { .. },
        )
    );
    if allowed {
        Ok(())
    } else {
        Err(Stage6DurableIdentityError::InvalidActionEvent)
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use broker_core::{Exchange, Market};
    use chrono::TimeZone;
    use rust_decimal::Decimal;
    use uuid::Uuid;

    fn request(n: u128) -> StrategyRequestId {
        StrategyRequestId::from(Uuid::from_u128((n << 96) | n))
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
    fn parsed_attribution(comment: &str) -> HybridRuntimeAttribution {
        HybridRuntimeAttribution::parse_source_comment(comment).unwrap()
    }
    fn place(kind: OrderType) -> PlaceOrder {
        let request_id = request(1);
        PlaceOrder {
            request_id,
            created_ts: Utc.with_ymd_and_hms(2026, 8, 9, 9, 0, 0).unwrap(),
            ttl_ms: Some(5000),
            account_id: account(),
            client_order_id: ClientOrderId::from_strategy_request(request_id),
            instrument: instrument(),
            side: OrderSide::Buy,
            order_type: kind,
            qty: Decimal::ONE,
            limit_price: (kind == OrderType::Limit).then(|| Decimal::new(2210, 1)),
            time_in_force: TimeInForce::Day,
            comment: Some(attribution("ENTRY").internal_comment().to_string()),
        }
    }
    fn evidence() -> Stage6Sha256Digest {
        Stage6Sha256Digest::parse("1".repeat(64)).unwrap()
    }
    fn accepted_place_record(kind: OrderType) -> Stage6JournalRecordV1 {
        let command = place(kind);
        let identity =
            Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap();
        let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, &command).unwrap();
        Stage6JournalRecordV1::request_accepted(
            identity,
            snapshot,
            Stage6LifecycleSequence::new(1).unwrap(),
            None,
            None,
            evidence(),
        )
        .unwrap()
    }
    fn cancel(role: &str) -> (CancelOrder, HybridRuntimeAttribution) {
        let command = CancelOrder {
            request_id: request(2),
            created_ts: Utc.with_ymd_and_hms(2026, 8, 9, 9, 1, 0).unwrap(),
            ttl_ms: Some(5000),
            account_id: account(),
            order_id: BrokerOrderId::new("ORDER/NON_NUMERIC"),
            client_order_id: Some(ClientOrderId::from_strategy_request(request(1))),
        };
        (command, attribution(role))
    }
    fn accepted_cancel_record() -> Stage6JournalRecordV1 {
        let (command, attribution) = cancel("CANCEL");
        let identity =
            Stage6DurableRequestIdentityV1::from_cancel(&command, instrument(), attribution)
                .unwrap();
        let snapshot = Stage6DurableCommandSnapshotV1::from_cancel(&identity, &command).unwrap();
        Stage6JournalRecordV1::request_accepted(
            identity,
            snapshot,
            Stage6LifecycleSequence::new(1).unwrap(),
            None,
            None,
            evidence(),
        )
        .unwrap()
    }
    fn place_with_comment(comment: &str) -> (PlaceOrder, HybridRuntimeAttribution) {
        let mut command = place(OrderType::Market);
        command.comment = Some(comment.to_string());
        (command, parsed_attribution(comment))
    }
    fn assert_reserved_event_rejected(event_kind: &str) {
        let mut value = serde_json::to_value(accepted_place_record(OrderType::Market)).unwrap();
        value["event_kind"] = serde_json::json!(event_kind);
        value["payload"] = serde_json::json!({"payload_kind": "marker"});
        value["canonical_payload_sha256"] =
            serde_json::json!(hex_sha256(&serde_json::to_vec(&value["payload"]).unwrap()));
        assert!(serde_json::from_value::<Stage6JournalRecordV1>(value).is_err());
    }

    fn assert_place_snapshot_error(command: &PlaceOrder, expected: Stage6DurableIdentityError) {
        let identity =
            Stage6DurableRequestIdentityV1::from_place(command, attribution("ENTRY")).unwrap();
        assert_eq!(
            Stage6DurableCommandSnapshotV1::from_place(&identity, command).unwrap_err(),
            expected
        );
    }

    fn assert_crafted_place_field_rejected(
        base_order_type: OrderType,
        field: &str,
        replacement: serde_json::Value,
    ) {
        let command = place(base_order_type);
        let identity =
            Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap();
        let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, &command).unwrap();
        let mut snapshot_value = serde_json::to_value(snapshot).unwrap();
        snapshot_value[field] = replacement.clone();
        assert!(serde_json::from_value::<Stage6DurableCommandSnapshotV1>(snapshot_value).is_err());

        let mut record_value =
            serde_json::to_value(accepted_place_record(base_order_type)).unwrap();
        record_value["payload"]["command"][field] = replacement;
        record_value["canonical_payload_sha256"] = serde_json::json!(hex_sha256(
            &serde_json::to_vec(&record_value["payload"]).unwrap()
        ));
        assert!(serde_json::from_value::<Stage6JournalRecordV1>(record_value.clone()).is_err());
        assert!(Stage6JournalRecordV1::decode_canonical(
            &serde_json::to_vec(&record_value).unwrap()
        )
        .is_err());
    }

    #[test]
    fn stage6a_identity_is_derived_and_stable() {
        let command = place(OrderType::Market);
        let first =
            Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap();
        let second =
            Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            first.durable_client_order_id(),
            &ClientOrderId::from_strategy_request(command.request_id)
        );
    }

    #[test]
    fn stage6a_cancel_keeps_durable_and_target_client_roles_separate() {
        let cancel_request = request(2);
        let target_client = ClientOrderId::from_strategy_request(request(1));
        let command = CancelOrder {
            request_id: cancel_request,
            created_ts: Utc.with_ymd_and_hms(2026, 8, 9, 9, 1, 0).unwrap(),
            ttl_ms: Some(5000),
            account_id: account(),
            order_id: BrokerOrderId::new("ORDER/NON_NUMERIC"),
            client_order_id: Some(target_client.clone()),
        };
        let identity = Stage6DurableRequestIdentityV1::from_cancel(
            &command,
            instrument(),
            attribution("CANCEL"),
        )
        .unwrap();
        assert_eq!(
            identity.durable_client_order_id(),
            &ClientOrderId::from_strategy_request(cancel_request)
        );
        assert_eq!(
            identity.target_order_client_order_id(),
            Some(&target_client)
        );
        assert_ne!(identity.durable_client_order_id(), &target_client);
        let snapshot = Stage6DurableCommandSnapshotV1::from_cancel(&identity, &command).unwrap();
        assert_eq!(snapshot.action(), Stage6DurableActionKind::Cancel);
    }

    #[test]
    fn stage6a_place_client_mismatch_is_rejected() {
        let mut command = place(OrderType::Market);
        command.client_order_id = ClientOrderId::from_strategy_request(request(9));
        assert_eq!(
            Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap_err(),
            Stage6DurableIdentityError::PlaceClientIdentityMismatch
        );
    }

    #[test]
    fn stage6a_fixture_request_ids_differ_inside_client_id_limit() {
        assert_ne!(
            ClientOrderId::from_strategy_request(request(1)),
            ClientOrderId::from_strategy_request(request(2))
        );
    }

    #[test]
    fn stage6a_market_limit_and_cancel_snapshots_are_reconstructable() {
        for kind in [OrderType::Market, OrderType::Limit] {
            let command = place(kind);
            let identity =
                Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap();
            let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, &command).unwrap();
            assert_eq!(snapshot.action(), Stage6DurableActionKind::Place);
        }
    }

    #[test]
    fn stage6a_market_snapshot_is_typed_place() {
        let command = place(OrderType::Market);
        let identity =
            Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap();
        assert_eq!(
            Stage6DurableCommandSnapshotV1::from_place(&identity, &command)
                .unwrap()
                .action(),
            Stage6DurableActionKind::Place
        );
    }

    #[test]
    fn stage6a_limit_snapshot_preserves_limit_price() {
        let text =
            String::from_utf8(accepted_place_record(OrderType::Limit).encode_canonical()).unwrap();
        assert!(
            text.contains("\"order_type\":\"Limit\"") && text.contains("\"limit_price\":\"221.0\"")
        );
    }

    #[test]
    fn stage6a_account_mismatch_is_rejected() {
        let command = place(OrderType::Market);
        let identity =
            Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap();
        let mut changed = command.clone();
        changed.account_id = BrokerAccountId::new("ACC_TEST_0002");
        assert_eq!(
            Stage6DurableCommandSnapshotV1::from_place(&identity, &changed).unwrap_err(),
            Stage6DurableIdentityError::AccountMismatch
        );
    }

    #[test]
    fn stage6a_instrument_mismatch_is_rejected() {
        let command = place(OrderType::Market);
        let identity =
            Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap();
        let mut changed = command.clone();
        changed.instrument.symbol = "RTS-9.26".into();
        assert_eq!(
            Stage6DurableCommandSnapshotV1::from_place(&identity, &changed).unwrap_err(),
            Stage6DurableIdentityError::InstrumentMismatch
        );
    }

    #[test]
    fn stage6a_record_id_is_payload_independent_and_sequence_sensitive() {
        let command = place(OrderType::Market);
        let identity =
            Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap();
        let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, &command).unwrap();
        let first = Stage6JournalRecordV1::request_accepted(
            identity.clone(),
            snapshot,
            Stage6LifecycleSequence::new(1).unwrap(),
            None,
            None,
            evidence(),
        )
        .unwrap();
        let second = Stage6JournalRecordV1::broker_order_observed(
            identity.clone(),
            BrokerOrderId::new("ORDER/OPAQUE"),
            Stage6LifecycleSequence::new(1).unwrap(),
            None,
            evidence(),
        )
        .unwrap();
        let third = Stage6JournalRecordV1::broker_order_observed(
            identity,
            BrokerOrderId::new("ORDER/OPAQUE"),
            Stage6LifecycleSequence::new(2).unwrap(),
            None,
            evidence(),
        )
        .unwrap();
        assert_eq!(first.journal_record_id(), second.journal_record_id());
        assert_ne!(
            first.canonical_payload_sha256(),
            second.canonical_payload_sha256()
        );
        assert_ne!(second.journal_record_id(), third.journal_record_id());
    }

    #[test]
    fn stage6a_sequence_zero_and_noncanonical_digest_are_rejected() {
        assert_eq!(
            Stage6LifecycleSequence::new(0).unwrap_err(),
            Stage6DurableIdentityError::SequenceZero
        );
        assert!(Stage6Sha256Digest::parse("").is_err());
        assert!(Stage6Sha256Digest::parse("A".repeat(64)).is_err());
        assert!(Stage6Sha256Digest::parse("0".repeat(64)).is_err());
    }

    #[test]
    fn stage6a_source_evidence_digest_is_preserved_exactly() {
        let record = accepted_place_record(OrderType::Market);
        assert_eq!(record.source_evidence_sha256().as_str(), "1".repeat(64));
    }

    #[test]
    fn stage6a_request_accepted_place_has_no_broker_id() {
        let value = serde_json::to_value(accepted_place_record(OrderType::Market)).unwrap();
        assert!(value["durable_request_identity"]["target_broker_order_id"].is_null());
        assert!(value["payload"].get("broker_order_id").is_none());
        assert!(value["payload"].get("broker_trade_id").is_none());
    }

    #[test]
    fn stage6a_broker_order_observation_requires_nonzero_evidence_type() {
        let command = place(OrderType::Market);
        let identity =
            Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap();
        let record = Stage6JournalRecordV1::broker_order_observed(
            identity,
            BrokerOrderId::new("ORDER/EXACT"),
            Stage6LifecycleSequence::new(2).unwrap(),
            None,
            evidence(),
        )
        .unwrap();
        assert!(String::from_utf8(record.encode_canonical())
            .unwrap()
            .contains("ORDER/EXACT"));
    }

    #[test]
    fn stage6a_forged_identity_deserialization_is_rejected() {
        let identity = Stage6DurableRequestIdentityV1::from_place(
            &place(OrderType::Market),
            attribution("ENTRY"),
        )
        .unwrap();
        let mut value = serde_json::to_value(identity).unwrap();
        value["durable_client_order_id"] = serde_json::json!("FORGED");
        assert!(serde_json::from_value::<Stage6DurableRequestIdentityV1>(value).is_err());
    }

    #[test]
    fn stage6a_payload_digest_tamper_is_rejected_by_generic_deserialization() {
        let mut value = serde_json::to_value(accepted_place_record(OrderType::Market)).unwrap();
        value["canonical_payload_sha256"] = serde_json::json!("2".repeat(64));
        assert!(serde_json::from_value::<Stage6JournalRecordV1>(value).is_err());
    }

    #[test]
    fn stage6a_record_id_tamper_is_rejected_by_generic_deserialization() {
        let mut value = serde_json::to_value(accepted_place_record(OrderType::Market)).unwrap();
        value["journal_record_id"] = serde_json::json!("2".repeat(64));
        assert!(serde_json::from_value::<Stage6JournalRecordV1>(value).is_err());
    }

    #[test]
    fn stage6a_all_zero_record_id_is_rejected() {
        assert!(
            serde_json::from_str::<Stage6JournalRecordId>(&format!("\"{}\"", "0".repeat(64)))
                .is_err()
        );
    }

    #[test]
    fn stage6a_uppercase_record_id_is_rejected() {
        assert!(
            serde_json::from_str::<Stage6JournalRecordId>(&format!("\"{}\"", "A".repeat(64)))
                .is_err()
        );
    }

    #[test]
    fn stage6a_schema_version_tamper_is_rejected() {
        let mut value = serde_json::to_value(accepted_place_record(OrderType::Market)).unwrap();
        value["schema_version"] = serde_json::json!(2);
        assert!(serde_json::from_value::<Stage6JournalRecordV1>(value).is_err());
    }

    #[test]
    fn stage6a_event_payload_mismatch_is_rejected() {
        let mut value = serde_json::to_value(accepted_place_record(OrderType::Market)).unwrap();
        value["event_kind"] = serde_json::json!("broker_order_observed");
        assert!(serde_json::from_value::<Stage6JournalRecordV1>(value).is_err());
    }

    #[test]
    fn stage6a_canonical_roundtrip_is_byte_exact() {
        let command = place(OrderType::Limit);
        let identity =
            Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap();
        let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, &command).unwrap();
        let record = Stage6JournalRecordV1::request_accepted(
            identity,
            snapshot,
            Stage6LifecycleSequence::new(1).unwrap(),
            None,
            None,
            evidence(),
        )
        .unwrap();
        let bytes = record.encode_canonical();
        let restored = Stage6JournalRecordV1::decode_canonical(&bytes).unwrap();
        assert_eq!(bytes, restored.encode_canonical());
    }

    #[test]
    fn stage6a_opaque_broker_order_and_trade_ids_survive_exactly() {
        let command = place(OrderType::Market);
        let identity =
            Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap();
        let record = Stage6JournalRecordV1::broker_trade_observed(
            identity,
            BrokerTradeId::new("TRADE/Ё/OPAQUE"),
            BrokerOrderId::new("ORDER/NON_NUMERIC"),
            Stage6LifecycleSequence::new(2).unwrap(),
            None,
            evidence(),
        )
        .unwrap();
        let bytes = record.encode_canonical();
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(text.contains("TRADE/Ё/OPAQUE") && text.contains("ORDER/NON_NUMERIC"));
        assert_eq!(
            bytes,
            Stage6JournalRecordV1::decode_canonical(&bytes)
                .unwrap()
                .encode_canonical()
        );
    }

    #[test]
    fn stage6a_place_and_cancel_records_match_exact_golden_bytes() {
        let place_command = place(OrderType::Limit);
        let place_identity =
            Stage6DurableRequestIdentityV1::from_place(&place_command, attribution("ENTRY"))
                .unwrap();
        let place_snapshot =
            Stage6DurableCommandSnapshotV1::from_place(&place_identity, &place_command).unwrap();
        let place_record = Stage6JournalRecordV1::request_accepted(
            place_identity,
            place_snapshot,
            Stage6LifecycleSequence::new(1).unwrap(),
            None,
            None,
            evidence(),
        )
        .unwrap();
        assert_eq!(
            place_record.encode_canonical(),
            include_bytes!("../../../fixtures/stage6a/place-request-accepted-v1.json")
                .strip_suffix(b"\n")
                .unwrap()
        );

        let cancel_command = CancelOrder {
            request_id: request(2),
            created_ts: Utc.with_ymd_and_hms(2026, 8, 9, 9, 1, 0).unwrap(),
            ttl_ms: Some(5000),
            account_id: account(),
            order_id: BrokerOrderId::new("ORDER/NON_NUMERIC"),
            client_order_id: Some(ClientOrderId::from_strategy_request(request(1))),
        };
        let cancel_identity = Stage6DurableRequestIdentityV1::from_cancel(
            &cancel_command,
            instrument(),
            attribution("CANCEL"),
        )
        .unwrap();
        let cancel_snapshot =
            Stage6DurableCommandSnapshotV1::from_cancel(&cancel_identity, &cancel_command).unwrap();
        let cancel_record = Stage6JournalRecordV1::request_accepted(
            cancel_identity,
            cancel_snapshot,
            Stage6LifecycleSequence::new(1).unwrap(),
            None,
            None,
            evidence(),
        )
        .unwrap();
        assert_eq!(
            cancel_record.encode_canonical(),
            include_bytes!("../../../fixtures/stage6a/cancel-request-accepted-v1.json")
                .strip_suffix(b"\n")
                .unwrap()
        );
    }

    #[test]
    fn stage6a_r1_place_identity_constructor_roundtrips() {
        let command = place(OrderType::Market);
        let identity =
            Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap();
        let encoded = serde_json::to_vec(&identity).unwrap();
        assert_eq!(identity, serde_json::from_slice(&encoded).unwrap());
    }

    #[test]
    fn stage6a_r1_cancel_identity_constructor_roundtrips() {
        let (command, attribution) = cancel("CANCEL");
        let identity =
            Stage6DurableRequestIdentityV1::from_cancel(&command, instrument(), attribution)
                .unwrap();
        let encoded = serde_json::to_vec(&identity).unwrap();
        assert_eq!(identity, serde_json::from_slice(&encoded).unwrap());
    }

    #[test]
    fn stage6a_r1_all_public_snapshot_constructors_roundtrip() {
        for kind in [OrderType::Market, OrderType::Limit] {
            let command = place(kind);
            let identity =
                Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap();
            let value = Stage6DurableCommandSnapshotV1::from_place(&identity, &command).unwrap();
            assert_eq!(
                value,
                serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap()
            );
        }
        let (command, attribution) = cancel("CANCEL");
        let identity =
            Stage6DurableRequestIdentityV1::from_cancel(&command, instrument(), attribution)
                .unwrap();
        let value = Stage6DurableCommandSnapshotV1::from_cancel(&identity, &command).unwrap();
        assert_eq!(
            value,
            serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap()
        );
    }

    #[test]
    fn stage6a_r1_all_public_record_constructors_roundtrip_exactly() {
        let place_record = accepted_place_record(OrderType::Market);
        let cancel_record = accepted_cancel_record();
        let command = place(OrderType::Market);
        let identity =
            Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap();
        let order = Stage6JournalRecordV1::broker_order_observed(
            identity.clone(),
            BrokerOrderId::new("ORDER/OPAQUE"),
            Stage6LifecycleSequence::new(2).unwrap(),
            None,
            evidence(),
        )
        .unwrap();
        let trade = Stage6JournalRecordV1::broker_trade_observed(
            identity,
            BrokerTradeId::new("TRADE/OPAQUE"),
            BrokerOrderId::new("ORDER/OPAQUE"),
            Stage6LifecycleSequence::new(3).unwrap(),
            Some(order.journal_record_id().clone()),
            evidence(),
        )
        .unwrap();
        let mut records = vec![place_record, cancel_record, order, trade];
        for comment in [
            "HYB|sid=hybrid_imoexf|c=cycle0001|o=BO|r=EXIT",
            "HYB|sid=hybrid_imoexf|c=cycle0001|o=MR|r=TP",
            "HYB|sid=hybrid_imoexf|c=cycle0001|o=MR|r=SL",
        ] {
            let (command, attribution) = place_with_comment(comment);
            let identity =
                Stage6DurableRequestIdentityV1::from_place(&command, attribution).unwrap();
            let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, &command).unwrap();
            records.push(
                Stage6JournalRecordV1::request_accepted(
                    identity,
                    snapshot,
                    Stage6LifecycleSequence::new(1).unwrap(),
                    None,
                    None,
                    evidence(),
                )
                .unwrap(),
            );
        }
        for record in records {
            let bytes = record.encode_canonical();
            assert_eq!(
                record,
                Stage6JournalRecordV1::decode_canonical(&bytes).unwrap()
            );
        }
    }

    #[test]
    fn stage6a_r1_entry_place_role_is_accepted() {
        assert!(Stage6DurableRequestIdentityV1::from_place(
            &place(OrderType::Market),
            attribution("ENTRY")
        )
        .is_ok());
    }

    #[test]
    fn stage6a_r1_exit_place_role_is_accepted() {
        let (command, attribution) =
            place_with_comment("HYB|sid=hybrid_imoexf|c=cycle0001|o=BO|r=EXIT");
        assert!(Stage6DurableRequestIdentityV1::from_place(&command, attribution).is_ok());
    }

    #[test]
    fn stage6a_r1_take_profit_place_role_is_accepted() {
        let (command, attribution) =
            place_with_comment("HYB|sid=hybrid_imoexf|c=cycle0001|o=MR|r=TP");
        assert!(Stage6DurableRequestIdentityV1::from_place(&command, attribution).is_ok());
    }

    #[test]
    fn stage6a_r1_stop_loss_place_role_is_accepted() {
        let (command, attribution) =
            place_with_comment("HYB|sid=hybrid_imoexf|c=cycle0001|o=MR|r=SL");
        assert!(Stage6DurableRequestIdentityV1::from_place(&command, attribution).is_ok());
    }

    #[test]
    fn stage6a_r1_empty_account_place_is_rejected() {
        let mut command = place(OrderType::Market);
        command.account_id = BrokerAccountId::new("");
        assert_eq!(
            Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap_err(),
            Stage6DurableIdentityError::InvalidAccountIdentity
        );
    }

    #[test]
    fn stage6a_r1_empty_account_cancel_is_rejected() {
        let (mut command, attribution) = cancel("CANCEL");
        command.account_id = BrokerAccountId::new("");
        assert_eq!(
            Stage6DurableRequestIdentityV1::from_cancel(&command, instrument(), attribution)
                .unwrap_err(),
            Stage6DurableIdentityError::InvalidAccountIdentity
        );
    }

    #[test]
    fn stage6a_r1_empty_instrument_symbol_place_is_rejected() {
        let mut command = place(OrderType::Market);
        command.instrument.symbol.clear();
        assert_eq!(
            Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap_err(),
            Stage6DurableIdentityError::InvalidInstrumentIdentity
        );
    }

    #[test]
    fn stage6a_r1_empty_instrument_symbol_cancel_is_rejected() {
        let (command, attribution) = cancel("CANCEL");
        let mut invalid = instrument();
        invalid.symbol.clear();
        assert_eq!(
            Stage6DurableRequestIdentityV1::from_cancel(&command, invalid, attribution)
                .unwrap_err(),
            Stage6DurableIdentityError::InvalidInstrumentIdentity
        );
    }

    #[test]
    fn stage6a_r1_empty_venue_symbol_is_rejected() {
        let mut command = place(OrderType::Market);
        command.instrument.venue_symbol = Some(String::new());
        assert_eq!(
            Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap_err(),
            Stage6DurableIdentityError::InvalidInstrumentIdentity
        );
    }

    #[test]
    fn stage6a_r1_empty_strategy_id_is_rejected_at_constructor() {
        let (command, attribution) = place_with_comment("HYB|sid=|c=cycle0001|o=BO|r=ENTRY");
        assert_eq!(
            Stage6DurableRequestIdentityV1::from_place(&command, attribution).unwrap_err(),
            Stage6DurableIdentityError::InvalidAttribution
        );
    }

    #[test]
    fn stage6a_r1_empty_cycle_id_is_rejected_at_constructor() {
        let (command, attribution) = place_with_comment("HYB|sid=hybrid_imoexf|c=|o=BO|r=ENTRY");
        assert_eq!(
            Stage6DurableRequestIdentityV1::from_place(&command, attribution).unwrap_err(),
            Stage6DurableIdentityError::InvalidAttribution
        );
    }

    #[test]
    fn stage6a_r1_missing_owner_is_rejected_at_constructor() {
        let (command, attribution) =
            place_with_comment("HYB|sid=hybrid_imoexf|c=cycle0001|o=UNKNOWN|r=ENTRY");
        assert_eq!(
            Stage6DurableRequestIdentityV1::from_place(&command, attribution).unwrap_err(),
            Stage6DurableIdentityError::InvalidAttribution
        );
    }

    #[test]
    fn stage6a_r1_cancel_missing_owner_is_rejected_at_constructor() {
        let (command, _) = cancel("CANCEL");
        let attribution =
            parsed_attribution("HYB|sid=hybrid_imoexf|c=cycle0001|o=UNKNOWN|r=CANCEL");
        assert_eq!(
            Stage6DurableRequestIdentityV1::from_cancel(&command, instrument(), attribution)
                .unwrap_err(),
            Stage6DurableIdentityError::InvalidAttribution
        );
    }

    #[test]
    fn stage6a_r1_missing_role_is_rejected_at_constructor() {
        let (command, attribution) =
            place_with_comment("HYB|sid=hybrid_imoexf|c=cycle0001|o=BO|r=UNKNOWN");
        assert_eq!(
            Stage6DurableRequestIdentityV1::from_place(&command, attribution).unwrap_err(),
            Stage6DurableIdentityError::InvalidAttribution
        );
    }

    #[test]
    fn stage6a_r1_place_cancel_role_is_rejected() {
        let (command, attribution) =
            place_with_comment("HYB|sid=hybrid_imoexf|c=cycle0001|o=BO|r=CANCEL");
        assert_eq!(
            Stage6DurableRequestIdentityV1::from_place(&command, attribution).unwrap_err(),
            Stage6DurableIdentityError::ActionRoleMismatch
        );
    }

    #[test]
    fn stage6a_r1_cancel_non_cancel_role_is_rejected() {
        let (command, _) = cancel("CANCEL");
        assert_eq!(
            Stage6DurableRequestIdentityV1::from_cancel(
                &command,
                instrument(),
                attribution("ENTRY")
            )
            .unwrap_err(),
            Stage6DurableIdentityError::ActionRoleMismatch
        );
    }

    #[test]
    fn stage6a_r1_canonical_decode_rejects_trailing_newline() {
        let mut bytes = accepted_place_record(OrderType::Market).encode_canonical();
        bytes.push(b'\n');
        assert_eq!(
            Stage6JournalRecordV1::decode_canonical(&bytes).unwrap_err(),
            Stage6DurableIdentityError::NonCanonicalEncoding
        );
    }

    #[test]
    fn stage6a_r1_canonical_decode_rejects_leading_whitespace() {
        let mut bytes = vec![b' '];
        bytes.extend(accepted_place_record(OrderType::Market).encode_canonical());
        assert_eq!(
            Stage6JournalRecordV1::decode_canonical(&bytes).unwrap_err(),
            Stage6DurableIdentityError::NonCanonicalEncoding
        );
    }

    #[test]
    fn stage6a_r1_canonical_decode_rejects_reordered_top_level_fields() {
        let value = serde_json::to_value(accepted_place_record(OrderType::Market)).unwrap();
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(Stage6JournalRecordV1::decode_canonical(&bytes).is_err());
    }

    #[test]
    fn stage6a_r1_canonical_decode_rejects_unknown_top_level_field() {
        let mut value = serde_json::to_value(accepted_place_record(OrderType::Market)).unwrap();
        value["unknown_stage6a"] = serde_json::json!(true);
        assert!(
            Stage6JournalRecordV1::decode_canonical(&serde_json::to_vec(&value).unwrap()).is_err()
        );
    }

    #[test]
    fn stage6a_r1_canonical_decode_rejects_unknown_identity_field() {
        let mut value = serde_json::to_value(accepted_place_record(OrderType::Market)).unwrap();
        value["durable_request_identity"]["unknown_stage6a"] = serde_json::json!(true);
        assert!(
            Stage6JournalRecordV1::decode_canonical(&serde_json::to_vec(&value).unwrap()).is_err()
        );
    }

    #[test]
    fn stage6a_r1_canonical_decode_rejects_unknown_payload_field() {
        let mut value = serde_json::to_value(accepted_place_record(OrderType::Market)).unwrap();
        value["payload"]["unknown_stage6a"] = serde_json::json!(true);
        assert!(
            Stage6JournalRecordV1::decode_canonical(&serde_json::to_vec(&value).unwrap()).is_err()
        );
    }

    #[test]
    fn stage6a_r1_canonical_decode_rejects_unknown_command_snapshot_field() {
        let mut value = serde_json::to_value(accepted_place_record(OrderType::Market)).unwrap();
        value["payload"]["command"]["unknown_stage6a"] = serde_json::json!(true);
        assert!(
            Stage6JournalRecordV1::decode_canonical(&serde_json::to_vec(&value).unwrap()).is_err()
        );
    }

    #[test]
    fn stage6a_r1_canonical_decode_rejects_equivalent_timestamp_spelling() {
        let bytes = accepted_place_record(OrderType::Market).encode_canonical();
        let changed = String::from_utf8(bytes)
            .unwrap()
            .replace("2026-08-09T09:00:00Z", "2026-08-09T09:00:00+00:00")
            .into_bytes();
        assert_eq!(
            Stage6JournalRecordV1::decode_canonical(&changed).unwrap_err(),
            Stage6DurableIdentityError::NonCanonicalEncoding
        );
    }

    #[test]
    fn stage6a_r1_dispatch_marker_is_rejected() {
        assert_reserved_event_rejected("dispatch_attempt_recorded");
    }

    #[test]
    fn stage6a_r1_cancel_outcome_marker_is_rejected() {
        assert_reserved_event_rejected("cancel_outcome_observed");
    }

    #[test]
    fn stage6a_r1_reconciliation_marker_is_rejected() {
        assert_reserved_event_rejected("reconciliation_observed");
    }

    #[test]
    fn stage6a_r1_request_finalized_marker_is_rejected() {
        assert_reserved_event_rejected("request_finalized");
    }

    #[test]
    fn stage6a_r1_conflict_marker_is_rejected() {
        assert_reserved_event_rejected("conflict_observed");
    }

    #[test]
    fn stage6a_r2_market_without_limit_price_is_accepted() {
        let command = place(OrderType::Market);
        let identity =
            Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap();
        Stage6DurableCommandSnapshotV1::from_place(&identity, &command).unwrap();
    }

    #[test]
    fn stage6a_r2_limit_with_positive_limit_price_is_accepted() {
        let command = place(OrderType::Limit);
        let identity =
            Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap();
        Stage6DurableCommandSnapshotV1::from_place(&identity, &command).unwrap();
    }

    #[test]
    fn stage6a_r2_market_public_snapshot_roundtrip_is_exact() {
        let command = place(OrderType::Market);
        let identity =
            Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap();
        let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, &command).unwrap();
        let encoded = serde_json::to_vec(&snapshot).unwrap();
        let decoded: Stage6DurableCommandSnapshotV1 = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(snapshot, decoded);
        assert_eq!(encoded, serde_json::to_vec(&decoded).unwrap());
    }

    #[test]
    fn stage6a_r2_limit_public_snapshot_roundtrip_is_exact() {
        let command = place(OrderType::Limit);
        let identity =
            Stage6DurableRequestIdentityV1::from_place(&command, attribution("ENTRY")).unwrap();
        let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, &command).unwrap();
        let encoded = serde_json::to_vec(&snapshot).unwrap();
        let decoded: Stage6DurableCommandSnapshotV1 = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(snapshot, decoded);
        assert_eq!(encoded, serde_json::to_vec(&decoded).unwrap());
    }

    #[test]
    fn stage6a_r2_stop_place_is_rejected() {
        assert_place_snapshot_error(
            &place(OrderType::Stop),
            Stage6DurableIdentityError::UnsupportedDurablePlaceOrderType,
        );
        assert_crafted_place_field_rejected(
            OrderType::Market,
            "order_type",
            serde_json::json!("Stop"),
        );
    }

    #[test]
    fn stage6a_r2_stop_limit_place_is_rejected() {
        assert_place_snapshot_error(
            &place(OrderType::StopLimit),
            Stage6DurableIdentityError::UnsupportedDurablePlaceOrderType,
        );
    }

    #[test]
    fn stage6a_r2_take_profit_place_is_rejected() {
        assert_place_snapshot_error(
            &place(OrderType::TakeProfit),
            Stage6DurableIdentityError::UnsupportedDurablePlaceOrderType,
        );
    }

    #[test]
    fn stage6a_r2_take_profit_limit_place_is_rejected() {
        assert_place_snapshot_error(
            &place(OrderType::TakeProfitLimit),
            Stage6DurableIdentityError::UnsupportedDurablePlaceOrderType,
        );
    }

    #[test]
    fn stage6a_r2_market_with_limit_price_is_rejected() {
        let mut command = place(OrderType::Market);
        command.limit_price = Some(Decimal::ONE);
        assert_place_snapshot_error(
            &command,
            Stage6DurableIdentityError::InvalidDurablePlacePriceShape,
        );
        assert_crafted_place_field_rejected(
            OrderType::Market,
            "limit_price",
            serde_json::json!("1"),
        );
    }

    #[test]
    fn stage6a_r2_limit_without_price_is_rejected() {
        let mut command = place(OrderType::Limit);
        command.limit_price = None;
        assert_place_snapshot_error(
            &command,
            Stage6DurableIdentityError::InvalidDurablePlacePriceShape,
        );
        assert_crafted_place_field_rejected(
            OrderType::Limit,
            "limit_price",
            serde_json::Value::Null,
        );
    }

    #[test]
    fn stage6a_r2_limit_with_zero_price_is_rejected() {
        let mut command = place(OrderType::Limit);
        command.limit_price = Some(Decimal::ZERO);
        assert_place_snapshot_error(
            &command,
            Stage6DurableIdentityError::InvalidDurablePlacePriceShape,
        );
    }

    #[test]
    fn stage6a_r2_limit_with_negative_price_is_rejected() {
        let mut command = place(OrderType::Limit);
        command.limit_price = Some(-Decimal::ONE);
        assert_place_snapshot_error(
            &command,
            Stage6DurableIdentityError::InvalidDurablePlacePriceShape,
        );
    }

    #[test]
    fn stage6a_r2_market_with_zero_quantity_is_rejected() {
        let mut command = place(OrderType::Market);
        command.qty = Decimal::ZERO;
        assert_place_snapshot_error(
            &command,
            Stage6DurableIdentityError::InvalidDurablePlaceQuantity,
        );
        assert_crafted_place_field_rejected(OrderType::Market, "quantity", serde_json::json!("0"));
    }

    #[test]
    fn stage6a_r2_market_with_negative_quantity_is_rejected() {
        let mut command = place(OrderType::Market);
        command.qty = -Decimal::ONE;
        assert_place_snapshot_error(
            &command,
            Stage6DurableIdentityError::InvalidDurablePlaceQuantity,
        );
    }

    #[test]
    fn stage6a_r2_limit_with_zero_quantity_is_rejected() {
        let mut command = place(OrderType::Limit);
        command.qty = Decimal::ZERO;
        assert_place_snapshot_error(
            &command,
            Stage6DurableIdentityError::InvalidDurablePlaceQuantity,
        );
    }

    #[test]
    fn stage6a_r2_limit_with_negative_quantity_is_rejected() {
        let mut command = place(OrderType::Limit);
        command.qty = -Decimal::ONE;
        assert_place_snapshot_error(
            &command,
            Stage6DurableIdentityError::InvalidDurablePlaceQuantity,
        );
    }

    #[test]
    fn stage6c_r1_crafted_cancel_no_order_record_is_rejected_on_decode() {
        let place_accepted = accepted_place_record(OrderType::Limit);
        let place_reconciliation = Stage6JournalRecordV1::reconciliation_observed(
            place_accepted.durable_request_identity().clone(),
            Stage6ReconciliationDispositionV1::NoBrokerOrderFound,
            Stage6LifecycleSequence::new(2).unwrap(),
            Some(place_accepted.journal_record_id().clone()),
            evidence(),
        )
        .unwrap();
        let cancel_accepted = accepted_cancel_record();
        let cancel_identity = cancel_accepted.durable_request_identity().clone();
        let sequence = Stage6LifecycleSequence::new(2).unwrap();
        let mut crafted: serde_json::Value =
            serde_json::from_slice(&place_reconciliation.encode_canonical()).unwrap();
        crafted["durable_request_identity"] = serde_json::to_value(&cancel_identity).unwrap();
        crafted["journal_record_id"] = serde_json::to_value(Stage6JournalRecordId::derive(
            cancel_identity.strategy_request_id(),
            sequence,
        ))
        .unwrap();
        crafted["previous_record_id"] =
            serde_json::to_value(cancel_accepted.journal_record_id()).unwrap();
        crafted["causal_parent_id"] =
            serde_json::to_value(cancel_accepted.journal_record_id()).unwrap();

        assert_eq!(
            Stage6JournalRecordV1::decode_canonical(&serde_json::to_vec(&crafted).unwrap())
                .unwrap_err(),
            Stage6DurableIdentityError::DecodeFailed
        );
    }
}
