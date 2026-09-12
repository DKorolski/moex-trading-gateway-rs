//! Stage 8A-4 I1 additive Stage 6 reconciliation record codec and mixed replay.
//!
//! This module is deliberately read-only. It exposes no V2 constructor, journal
//! append, durable apply, transport, Redis, ACK or readiness authority.
//!
//! A caller cannot bypass canonical validation through generic deserialization:
//!
//! ```compile_fail,E0277
//! use strategy_runtime_core::Stage6JournalRecordV2;
//! let _: Stage6JournalRecordV2 = serde_json::from_slice(b"{}").unwrap();
//! ```
//!
//! The version-aware surface is a reader and has no append authority:
//!
//! ```compile_fail,E0599
//! use strategy_runtime_core::Stage6VersionedJournalReader;
//! let _ = Stage6VersionedJournalReader::append(b"{}");
//! ```

use crate::stage6_replay::WorkingRequest;
use crate::{
    Stage6CancelOutcomeV1, Stage6DurableActionKind, Stage6DurableIdentityError,
    Stage6DurablePlaceOrderShapeV1, Stage6DurableRequestIdentityV1, Stage6JournalEventKind,
    Stage6JournalRecordId, Stage6JournalRecordV1, Stage6LifecycleSequence,
    Stage6RecoveredRequestV1, Stage6ReplayError, Stage6RequestFinalDispositionV1,
    Stage6Sha256Digest,
};
use broker_core::{
    BrokerAccountId, BrokerOrderId, BrokerOrderLifecycle, BrokerTradeId, ClientOrderId,
    InstrumentId, OrderSide, OrderStatus, OrderType, Price, Quantity, TimeInForce,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::Digest;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const STAGE6_DURABLE_RECORD_SCHEMA_VERSION_V2: u16 = 2;
pub const STAGE6_DURABLE_RECORD_SCHEMA_VERSION_V3: u16 = 3;
pub const STAGE6_DURABLE_RECORD_SCHEMA_VERSION_V4: u16 = 4;
const MAX_MATERIAL_TRADES_V2: usize = 256;
const MAX_SUFFIX_RECORDS_V2: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage6ReconciliationV2Error {
    DecodeFailed,
    UnsupportedSchema(u64),
    AmbiguousSchema,
    NonCanonicalEncoding,
    RecordIdentityMismatch,
    PayloadDigestMismatch,
    EventPayloadMismatch,
    InvalidCausalEnvelope,
    InvalidDurableIdentity,
    InvalidPayload,
    InvalidLookupEvidence,
    InvalidBrokerOrderFact,
    InvalidMaterialTradeFact,
    InvalidSuffixManifest,
    CollectionBoundExceeded,
    Replay(Stage6ReplayError),
    PendingBatchConflict,
    UnexpectedSuffixRecord,
    V2AfterFinalization,
}

impl fmt::Display for Stage6ReconciliationV2Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DecodeFailed => formatter.write_str("Stage 6 record decode failed"),
            Self::UnsupportedSchema(value) => {
                write!(formatter, "unsupported Stage 6 record schema {value}")
            }
            Self::AmbiguousSchema => formatter.write_str("ambiguous Stage 6 record schema"),
            Self::NonCanonicalEncoding => {
                formatter.write_str("non-canonical Stage 6 record encoding")
            }
            Self::RecordIdentityMismatch => {
                formatter.write_str("Stage 6 V2 record identity mismatch")
            }
            Self::PayloadDigestMismatch => {
                formatter.write_str("Stage 6 V2 payload digest mismatch")
            }
            Self::EventPayloadMismatch => formatter.write_str("Stage 6 V2 event/payload mismatch"),
            Self::InvalidCausalEnvelope => {
                formatter.write_str("invalid Stage 6 V2 causal envelope")
            }
            Self::InvalidDurableIdentity => {
                formatter.write_str("invalid Stage 6 V2 durable identity")
            }
            Self::InvalidPayload => {
                formatter.write_str("invalid Stage 6 V2 reconciliation payload")
            }
            Self::InvalidLookupEvidence => {
                formatter.write_str("invalid Stage 6 V2 exact lookup evidence")
            }
            Self::InvalidBrokerOrderFact => {
                formatter.write_str("invalid Stage 6 V2 broker order fact")
            }
            Self::InvalidMaterialTradeFact => {
                formatter.write_str("invalid Stage 6 V2 material trade fact")
            }
            Self::InvalidSuffixManifest => {
                formatter.write_str("invalid Stage 6 V2 suffix manifest")
            }
            Self::CollectionBoundExceeded => {
                formatter.write_str("Stage 6 V2 collection bound exceeded")
            }
            Self::Replay(error) => write!(formatter, "Stage 6 mixed replay failed: {error}"),
            Self::PendingBatchConflict => {
                formatter.write_str("Stage 6 pending reconciliation batch conflict")
            }
            Self::UnexpectedSuffixRecord => {
                formatter.write_str("unexpected Stage 6 V1 suffix record")
            }
            Self::V2AfterFinalization => {
                formatter.write_str("Stage 6 V2 transition follows finalization")
            }
        }
    }
}

impl std::error::Error for Stage6ReconciliationV2Error {}

impl From<Stage6ReplayError> for Stage6ReconciliationV2Error {
    fn from(value: Stage6ReplayError) -> Self {
        Self::Replay(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage6ReconciliationEndpointKindV2 {
    Place,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage6ReconciliationLifecycleV2 {
    Working,
    TerminalFilled,
    TerminalRejected,
    TerminalCancelled,
    TerminalExpired,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Stage6ReconciliationTransitionKindV2 {
    Exact {
        lifecycle: Stage6ReconciliationLifecycleV2,
    },
    ReconciliationConflictHold,
    ReconciliationStillUnknownHold,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Stage6ReconciliationFillEffectV2 {
    Zero,
    Partial { filled_qty: Quantity },
    Full { filled_qty: Quantity },
}

impl Stage6ReconciliationFillEffectV2 {
    fn validate(&self) -> Result<(), Stage6ReconciliationV2Error> {
        match self {
            Self::Zero => Ok(()),
            Self::Partial { filled_qty } | Self::Full { filled_qty }
                if *filled_qty > Quantity::ZERO =>
            {
                Ok(())
            }
            _ => Err(Stage6ReconciliationV2Error::InvalidPayload),
        }
    }
}

fn validate_exact_state_matrix(
    declared_lifecycle: Stage6ReconciliationLifecycleV2,
    order: &Stage6BrokerOrderFactV2,
    fill_effect: &Stage6ReconciliationFillEffectV2,
) -> Result<(), Stage6ReconciliationV2Error> {
    // BEGIN EXACT STATUS FILL MATRIX
    let derived_lifecycle = match (&order.status, fill_effect) {
        (OrderStatus::New | OrderStatus::Working, Stage6ReconciliationFillEffectV2::Zero)
            if order.filled_qty == Quantity::ZERO =>
        {
            Stage6ReconciliationLifecycleV2::Working
        }
        (
            OrderStatus::PartiallyFilled,
            Stage6ReconciliationFillEffectV2::Partial { filled_qty },
        ) if *filled_qty == order.filled_qty
            && *filled_qty > Quantity::ZERO
            && *filled_qty < order.qty =>
        {
            Stage6ReconciliationLifecycleV2::Working
        }
        (OrderStatus::Filled, Stage6ReconciliationFillEffectV2::Full { filled_qty })
            if *filled_qty == order.filled_qty && *filled_qty == order.qty =>
        {
            Stage6ReconciliationLifecycleV2::TerminalFilled
        }
        (OrderStatus::Rejected, Stage6ReconciliationFillEffectV2::Zero)
            if order.filled_qty == Quantity::ZERO =>
        {
            Stage6ReconciliationLifecycleV2::TerminalRejected
        }
        (OrderStatus::Canceled, Stage6ReconciliationFillEffectV2::Zero)
            if order.filled_qty == Quantity::ZERO =>
        {
            Stage6ReconciliationLifecycleV2::TerminalCancelled
        }
        (OrderStatus::Canceled, Stage6ReconciliationFillEffectV2::Partial { filled_qty })
            if *filled_qty == order.filled_qty
                && *filled_qty > Quantity::ZERO
                && *filled_qty < order.qty =>
        {
            Stage6ReconciliationLifecycleV2::TerminalCancelled
        }
        (OrderStatus::Expired, Stage6ReconciliationFillEffectV2::Zero)
            if order.filled_qty == Quantity::ZERO =>
        {
            Stage6ReconciliationLifecycleV2::TerminalExpired
        }
        (OrderStatus::Expired, Stage6ReconciliationFillEffectV2::Partial { filled_qty })
            if *filled_qty == order.filled_qty
                && *filled_qty > Quantity::ZERO
                && *filled_qty < order.qty =>
        {
            Stage6ReconciliationLifecycleV2::TerminalExpired
        }
        _ => return Err(Stage6ReconciliationV2Error::InvalidPayload),
    };
    // END EXACT STATUS FILL MATRIX
    if declared_lifecycle != derived_lifecycle {
        return Err(Stage6ReconciliationV2Error::InvalidPayload);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage6BrokerOrderFactV2 {
    account_id: BrokerAccountId,
    broker_order_id: Option<BrokerOrderId>,
    client_order_id: Option<ClientOrderId>,
    instrument: InstrumentId,
    side: OrderSide,
    order_type: OrderType,
    time_in_force: Option<TimeInForce>,
    status: OrderStatus,
    lifecycle: BrokerOrderLifecycle,
    qty: Quantity,
    filled_qty: Quantity,
    remaining_qty: Option<Quantity>,
    limit_price: Option<Price>,
    broker_asset_id: Option<String>,
    board: Option<String>,
    expiration_date: Option<NaiveDate>,
    source_ts: Option<DateTime<Utc>>,
    received_ts: DateTime<Utc>,
}

impl Stage6BrokerOrderFactV2 {
    pub fn broker_order_id(&self) -> Option<&BrokerOrderId> {
        self.broker_order_id.as_ref()
    }
    pub fn client_order_id(&self) -> Option<&ClientOrderId> {
        self.client_order_id.as_ref()
    }

    pub fn matches_original_place_shape(&self, shape: &Stage6DurablePlaceOrderShapeV1) -> bool {
        self.time_in_force.is_some_and(|time_in_force| {
            shape.matches(
                self.side,
                self.order_type,
                self.qty,
                self.limit_price,
                time_in_force,
            )
        })
    }

    fn validate(
        &self,
        identity: &Stage6DurableRequestIdentityV1,
    ) -> Result<(), Stage6ReconciliationV2Error> {
        let expected_lifecycle = match self.status {
            OrderStatus::New | OrderStatus::Working | OrderStatus::PartiallyFilled => {
                BrokerOrderLifecycle::Active
            }
            OrderStatus::Filled
            | OrderStatus::Canceled
            | OrderStatus::Rejected
            | OrderStatus::Expired => BrokerOrderLifecycle::Terminal,
            OrderStatus::Unknown(_) => BrokerOrderLifecycle::Unknown,
        };
        if &self.account_id != identity.account_id()
            || &self.instrument != identity.instrument()
            || self.qty <= Quantity::ZERO
            || self.filled_qty < Quantity::ZERO
            || self.filled_qty > self.qty
            || self.lifecycle != expected_lifecycle
            || self.remaining_qty.is_some_and(|remaining| {
                remaining < Quantity::ZERO || self.filled_qty + remaining != self.qty
            })
            || self.broker_asset_id.as_deref().is_some_and(str::is_empty)
            || self.board.as_deref().is_some_and(str::is_empty)
            || self
                .source_ts
                .is_some_and(|source| source > self.received_ts)
        {
            return Err(Stage6ReconciliationV2Error::InvalidBrokerOrderFact);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage6MaterialTradeFactV2 {
    account_id: BrokerAccountId,
    broker_trade_id: BrokerTradeId,
    broker_order_id: Option<BrokerOrderId>,
    client_order_id: Option<ClientOrderId>,
    instrument: InstrumentId,
    side: OrderSide,
    qty: Quantity,
    price: Price,
    gross_amount: Option<rust_decimal::Decimal>,
    commission: Option<rust_decimal::Decimal>,
    broker_asset_id: Option<String>,
    board: Option<String>,
    expiration_date: Option<NaiveDate>,
    source_ts: DateTime<Utc>,
    received_ts: DateTime<Utc>,
}

impl Stage6MaterialTradeFactV2 {
    pub fn broker_trade_id(&self) -> &BrokerTradeId {
        &self.broker_trade_id
    }
    pub fn broker_order_id(&self) -> Option<&BrokerOrderId> {
        self.broker_order_id.as_ref()
    }
    pub fn client_order_id(&self) -> Option<&ClientOrderId> {
        self.client_order_id.as_ref()
    }

    fn validate(
        &self,
        identity: &Stage6DurableRequestIdentityV1,
    ) -> Result<(), Stage6ReconciliationV2Error> {
        if &self.account_id != identity.account_id()
            || &self.instrument != identity.instrument()
            || self.qty <= Quantity::ZERO
            || self.price <= Price::ZERO
            || self.broker_asset_id.as_deref().is_some_and(str::is_empty)
            || self.board.as_deref().is_some_and(str::is_empty)
            || self.source_ts > self.received_ts
        {
            return Err(Stage6ReconciliationV2Error::InvalidMaterialTradeFact);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage6AccountSafetySummaryV2 {
    account_active_orders_count: u32,
    account_unknown_orders_count: u32,
    account_orphan_orders_count: u32,
    account_open_positions_count: u32,
    target_active_orders_count: u32,
    target_unknown_orders_count: u32,
    target_terminal_orders_count: u32,
    target_inconsistent_orders_count: u32,
    target_open_positions_count: u32,
    other_symbol_active_orders_count: u32,
    account_safety_binding_sha256: Stage6Sha256Digest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage6PreAppendPreconditionV2 {
    expected_stage6_checkpoint_or_frontier_fingerprint: Stage6Sha256Digest,
    expected_recovery_seal_generation: u64,
    expected_recovery_seal_fingerprint: Stage6Sha256Digest,
    expected_request_state_fingerprint: Stage6Sha256Digest,
}

impl Stage6PreAppendPreconditionV2 {
    pub fn expected_stage6_checkpoint_or_frontier_fingerprint(&self) -> &Stage6Sha256Digest {
        &self.expected_stage6_checkpoint_or_frontier_fingerprint
    }

    pub fn expected_recovery_seal_generation(&self) -> u64 {
        self.expected_recovery_seal_generation
    }

    pub fn expected_recovery_seal_fingerprint(&self) -> &Stage6Sha256Digest {
        &self.expected_recovery_seal_fingerprint
    }

    pub fn expected_request_state_fingerprint(&self) -> &Stage6Sha256Digest {
        &self.expected_request_state_fingerprint
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage6ExactOrderObservationV2 {
    order: Stage6BrokerOrderFactV2,
    observation_binding_sha256: Stage6Sha256Digest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
// The accepted R2 wire contract owns the successful observation inline. Boxing
// it would change the frozen DTO shape merely to optimize process-local layout.
#[allow(clippy::large_enum_variant)]
pub enum Stage6ExactLookupEvidenceV2 {
    NotAttempted,
    Succeeded {
        account_id: BrokerAccountId,
        queried_broker_order_id: BrokerOrderId,
        durable_request_binding_sha256: Stage6Sha256Digest,
        request_started_at: DateTime<Utc>,
        response_received_at: DateTime<Utc>,
        exact_order_observation_v2: Stage6ExactOrderObservationV2,
    },
    DocumentedNotFound {
        account_id: BrokerAccountId,
        queried_broker_order_id: BrokerOrderId,
        durable_request_binding_sha256: Stage6Sha256Digest,
        request_started_at: DateTime<Utc>,
        response_received_at: DateTime<Utc>,
        documented_status_category: String,
    },
    Unavailable {
        account_id: BrokerAccountId,
        queried_broker_order_id: BrokerOrderId,
        durable_request_binding_sha256: Stage6Sha256Digest,
        request_started_at: DateTime<Utc>,
        response_received_at: DateTime<Utc>,
        failure_category: String,
    },
    DecodeFailure {
        account_id: BrokerAccountId,
        queried_broker_order_id: BrokerOrderId,
        durable_request_binding_sha256: Stage6Sha256Digest,
        request_started_at: DateTime<Utc>,
        response_received_at: DateTime<Utc>,
        response_status_category: String,
        response_binding_sha256: Stage6Sha256Digest,
    },
    Stale {
        account_id: BrokerAccountId,
        queried_broker_order_id: BrokerOrderId,
        durable_request_binding_sha256: Stage6Sha256Digest,
        request_started_at: DateTime<Utc>,
        response_received_at: DateTime<Utc>,
        stale_observation_binding_sha256: Stage6Sha256Digest,
    },
}

impl Stage6ExactLookupEvidenceV2 {
    fn validate(
        &self,
        identity: &Stage6DurableRequestIdentityV1,
        expected_durable_request_binding: &Stage6Sha256Digest,
    ) -> Result<(), Stage6ReconciliationV2Error> {
        let attempted = match self {
            Self::NotAttempted => return Ok(()),
            Self::Succeeded {
                account_id,
                queried_broker_order_id,
                durable_request_binding_sha256,
                request_started_at,
                response_received_at,
                exact_order_observation_v2,
                ..
            } => {
                exact_order_observation_v2.order.validate(identity)?;
                if exact_order_observation_v2.order.broker_order_id.as_ref()
                    != Some(queried_broker_order_id)
                {
                    return Err(Stage6ReconciliationV2Error::InvalidLookupEvidence);
                }
                (
                    account_id,
                    durable_request_binding_sha256,
                    request_started_at,
                    response_received_at,
                    None,
                )
            }
            Self::DocumentedNotFound {
                account_id,
                durable_request_binding_sha256,
                request_started_at,
                response_received_at,
                documented_status_category,
                ..
            } => (
                account_id,
                durable_request_binding_sha256,
                request_started_at,
                response_received_at,
                Some(documented_status_category.as_str()),
            ),
            Self::Unavailable {
                account_id,
                durable_request_binding_sha256,
                request_started_at,
                response_received_at,
                failure_category,
                ..
            } => (
                account_id,
                durable_request_binding_sha256,
                request_started_at,
                response_received_at,
                Some(failure_category.as_str()),
            ),
            Self::DecodeFailure {
                account_id,
                durable_request_binding_sha256,
                request_started_at,
                response_received_at,
                response_status_category,
                ..
            } => (
                account_id,
                durable_request_binding_sha256,
                request_started_at,
                response_received_at,
                Some(response_status_category.as_str()),
            ),
            Self::Stale {
                account_id,
                durable_request_binding_sha256,
                request_started_at,
                response_received_at,
                ..
            } => (
                account_id,
                durable_request_binding_sha256,
                request_started_at,
                response_received_at,
                None,
            ),
        };
        if attempted.0 != identity.account_id()
            || attempted.1 != expected_durable_request_binding
            || attempted.2 > attempted.3
            || attempted.4.is_some_and(str::is_empty)
        {
            return Err(Stage6ReconciliationV2Error::InvalidLookupEvidence);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage6SuffixManifestEntryV2 {
    ordinal: u16,
    event_kind: Stage6JournalEventKind,
    journal_record_id: Stage6JournalRecordId,
    lifecycle_sequence: Stage6LifecycleSequence,
    canonical_payload_sha256: Stage6Sha256Digest,
    canonical_record_sha256: Stage6Sha256Digest,
}

impl Stage6SuffixManifestEntryV2 {
    pub fn matches_record(&self, record: &Stage6JournalRecordV1) -> bool {
        self.event_kind == record.event_kind()
            && self.journal_record_id == *record.journal_record_id()
            && self.lifecycle_sequence == record.lifecycle_sequence()
            && self.canonical_payload_sha256 == *record.canonical_payload_sha256()
            && self.canonical_record_sha256 == Stage6Sha256Digest::of(&record.encode_canonical())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage6SuffixManifestV2 {
    entries: Vec<Stage6SuffixManifestEntryV2>,
}

impl Stage6SuffixManifestV2 {
    pub fn entries(&self) -> &[Stage6SuffixManifestEntryV2] {
        &self.entries
    }

    fn validate(
        &self,
        identity: &Stage6DurableRequestIdentityV1,
        v2_sequence: Stage6LifecycleSequence,
    ) -> Result<(), Stage6ReconciliationV2Error> {
        if self.entries.len() > MAX_SUFFIX_RECORDS_V2 {
            return Err(Stage6ReconciliationV2Error::CollectionBoundExceeded);
        }
        for (index, entry) in self.entries.iter().enumerate() {
            let expected_sequence = v2_sequence
                .get()
                .checked_add(index as u64 + 1)
                .ok_or(Stage6ReconciliationV2Error::InvalidSuffixManifest)?;
            if usize::from(entry.ordinal) != index
                || entry.lifecycle_sequence.get() != expected_sequence
                || entry.journal_record_id
                    != Stage6JournalRecordId::derive(
                        identity.strategy_request_id(),
                        entry.lifecycle_sequence,
                    )
            {
                return Err(Stage6ReconciliationV2Error::InvalidSuffixManifest);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage6ReconciliationTransitionPayloadV2 {
    stable_transition_key_sha256: Stage6Sha256Digest,
    durable_request_binding_sha256: Stage6Sha256Digest,
    private_authoritative_outcome_binding_sha256: Stage6Sha256Digest,
    endpoint_kind: Stage6ReconciliationEndpointKindV2,
    transition_kind: Stage6ReconciliationTransitionKindV2,
    exact_lookup_evidence: Stage6ExactLookupEvidenceV2,
    broker_order_fact: Option<Stage6BrokerOrderFactV2>,
    material_trade_facts: Vec<Stage6MaterialTradeFactV2>,
    fill_effect: Stage6ReconciliationFillEffectV2,
    account_safety_summary: Stage6AccountSafetySummaryV2,
    pre_append_precondition: Stage6PreAppendPreconditionV2,
    deterministic_suffix_manifest: Stage6SuffixManifestV2,
}

impl Stage6ReconciliationTransitionPayloadV2 {
    pub fn stable_transition_key_sha256(&self) -> &Stage6Sha256Digest {
        &self.stable_transition_key_sha256
    }
    pub fn transition_kind(&self) -> &Stage6ReconciliationTransitionKindV2 {
        &self.transition_kind
    }
    pub fn durable_request_binding_sha256(&self) -> &Stage6Sha256Digest {
        &self.durable_request_binding_sha256
    }
    pub fn broker_order_fact(&self) -> Option<&Stage6BrokerOrderFactV2> {
        self.broker_order_fact.as_ref()
    }
    pub fn material_trade_facts(&self) -> &[Stage6MaterialTradeFactV2] {
        &self.material_trade_facts
    }
    pub fn suffix_manifest(&self) -> &Stage6SuffixManifestV2 {
        &self.deterministic_suffix_manifest
    }
    pub fn pre_append_precondition(&self) -> &Stage6PreAppendPreconditionV2 {
        &self.pre_append_precondition
    }
    pub fn account_safety_summary(&self) -> &Stage6AccountSafetySummaryV2 {
        &self.account_safety_summary
    }

    fn validate(
        &self,
        identity: &Stage6DurableRequestIdentityV1,
        sequence: Stage6LifecycleSequence,
    ) -> Result<(), Stage6ReconciliationV2Error> {
        if self.material_trade_facts.len() > MAX_MATERIAL_TRADES_V2
            || matches!(
                (identity.action(), self.endpoint_kind),
                (
                    Stage6DurableActionKind::Place,
                    Stage6ReconciliationEndpointKindV2::Cancel
                ) | (
                    Stage6DurableActionKind::Cancel,
                    Stage6ReconciliationEndpointKindV2::Place
                )
            )
        {
            return Err(Stage6ReconciliationV2Error::InvalidPayload);
        }
        self.fill_effect.validate()?;
        self.exact_lookup_evidence
            .validate(identity, &self.durable_request_binding_sha256)?;
        if let Some(order) = &self.broker_order_fact {
            order.validate(identity)?;
        }
        let mut trade_ids = BTreeSet::new();
        for trade in &self.material_trade_facts {
            trade.validate(identity)?;
            if !trade_ids.insert(trade.broker_trade_id.as_str()) {
                return Err(Stage6ReconciliationV2Error::InvalidMaterialTradeFact);
            }
        }
        if let Stage6ReconciliationTransitionKindV2::Exact { lifecycle } = self.transition_kind {
            let order = self
                .broker_order_fact
                .as_ref()
                .ok_or(Stage6ReconciliationV2Error::InvalidPayload)?;
            validate_exact_state_matrix(lifecycle, order, &self.fill_effect)?;
            let material_qty: Quantity = self
                .material_trade_facts
                .iter()
                .map(|trade| trade.qty)
                .sum();
            if material_qty != order.filled_qty {
                return Err(Stage6ReconciliationV2Error::InvalidPayload);
            }
        }
        self.deterministic_suffix_manifest
            .validate(identity, sequence)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage6JournalEventKindV2 {
    ReconciliationTransitionApplied,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stage6JournalRecordV2 {
    schema_version: u16,
    journal_record_id: Stage6JournalRecordId,
    lifecycle_sequence: Stage6LifecycleSequence,
    previous_record_id: Option<Stage6JournalRecordId>,
    causal_parent_id: Option<Stage6JournalRecordId>,
    durable_request_identity: Stage6DurableRequestIdentityV1,
    event_kind: Stage6JournalEventKindV2,
    payload: Stage6ReconciliationTransitionPayloadV2,
    canonical_payload_sha256: Stage6Sha256Digest,
    source_evidence_sha256: Stage6Sha256Digest,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage6JournalRecordWireV2 {
    schema_version: u16,
    journal_record_id: Stage6JournalRecordId,
    lifecycle_sequence: Stage6LifecycleSequence,
    previous_record_id: Option<Stage6JournalRecordId>,
    causal_parent_id: Option<Stage6JournalRecordId>,
    durable_request_identity: Stage6DurableRequestIdentityV1,
    event_kind: Stage6JournalEventKindV2,
    payload: Stage6ReconciliationTransitionPayloadV2,
    canonical_payload_sha256: Stage6Sha256Digest,
    source_evidence_sha256: Stage6Sha256Digest,
}

impl From<Stage6JournalRecordWireV2> for Stage6JournalRecordV2 {
    fn from(wire: Stage6JournalRecordWireV2) -> Self {
        Self {
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
        }
    }
}

impl Stage6JournalRecordV2 {
    pub fn encode_canonical(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("fixed V2 record serializes")
    }
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Stage6ReconciliationV2Error> {
        let wire: Stage6JournalRecordWireV2 =
            serde_json::from_slice(bytes).map_err(|_| Stage6ReconciliationV2Error::DecodeFailed)?;
        let value = Self::from(wire);
        value.validate()?;
        if value.encode_canonical() != bytes {
            return Err(Stage6ReconciliationV2Error::NonCanonicalEncoding);
        }
        Ok(value)
    }
    pub fn journal_record_id(&self) -> &Stage6JournalRecordId {
        &self.journal_record_id
    }
    pub fn lifecycle_sequence(&self) -> Stage6LifecycleSequence {
        self.lifecycle_sequence
    }
    pub fn previous_record_id(&self) -> Option<&Stage6JournalRecordId> {
        self.previous_record_id.as_ref()
    }
    pub fn causal_parent_id(&self) -> Option<&Stage6JournalRecordId> {
        self.causal_parent_id.as_ref()
    }
    pub fn durable_request_identity(&self) -> &Stage6DurableRequestIdentityV1 {
        &self.durable_request_identity
    }
    pub fn payload(&self) -> &Stage6ReconciliationTransitionPayloadV2 {
        &self.payload
    }
    pub fn canonical_record_sha256(&self) -> Stage6Sha256Digest {
        Stage6Sha256Digest::of(&self.encode_canonical())
    }
    pub(crate) fn source_evidence_sha256(&self) -> &Stage6Sha256Digest {
        &self.source_evidence_sha256
    }

    fn validate(&self) -> Result<(), Stage6ReconciliationV2Error> {
        if self.schema_version != STAGE6_DURABLE_RECORD_SCHEMA_VERSION_V2 {
            return Err(Stage6ReconciliationV2Error::UnsupportedSchema(u64::from(
                self.schema_version,
            )));
        }
        if self.previous_record_id.is_none() || self.causal_parent_id != self.previous_record_id {
            return Err(Stage6ReconciliationV2Error::InvalidCausalEnvelope);
        }
        if self.journal_record_id
            != Stage6JournalRecordId::derive(
                self.durable_request_identity.strategy_request_id(),
                self.lifecycle_sequence,
            )
        {
            return Err(Stage6ReconciliationV2Error::RecordIdentityMismatch);
        }
        self.durable_request_identity
            .validate_self()
            .map_err(|_| Stage6ReconciliationV2Error::InvalidDurableIdentity)?;
        if self.event_kind != Stage6JournalEventKindV2::ReconciliationTransitionApplied {
            return Err(Stage6ReconciliationV2Error::EventPayloadMismatch);
        }
        let digest = Stage6Sha256Digest::of(
            &serde_json::to_vec(&self.payload)
                .map_err(|_| Stage6ReconciliationV2Error::DecodeFailed)?,
        );
        if digest != self.canonical_payload_sha256 {
            return Err(Stage6ReconciliationV2Error::PayloadDigestMismatch);
        }
        self.payload
            .validate(&self.durable_request_identity, self.lifecycle_sequence)
    }

    #[cfg(test)]
    fn build_for_test(
        identity: Stage6DurableRequestIdentityV1,
        sequence: Stage6LifecycleSequence,
        previous: Stage6JournalRecordId,
        payload: Stage6ReconciliationTransitionPayloadV2,
        source_evidence_sha256: Stage6Sha256Digest,
    ) -> Self {
        let canonical_payload_sha256 =
            Stage6Sha256Digest::of(&serde_json::to_vec(&payload).unwrap());
        let value = Self {
            schema_version: STAGE6_DURABLE_RECORD_SCHEMA_VERSION_V2,
            journal_record_id: Stage6JournalRecordId::derive(
                identity.strategy_request_id(),
                sequence,
            ),
            lifecycle_sequence: sequence,
            previous_record_id: Some(previous.clone()),
            causal_parent_id: Some(previous),
            durable_request_identity: identity,
            event_kind: Stage6JournalEventKindV2::ReconciliationTransitionApplied,
            payload,
            canonical_payload_sha256,
            source_evidence_sha256,
        };
        value.validate().unwrap();
        value
    }
}

pub(crate) fn reconstruct_stage8a4_suffix_from_v2(
    transition: &Stage6JournalRecordV2,
) -> Result<
    (
        Vec<Stage6JournalRecordV1>,
        Option<Stage6DurablePlaceOrderShapeV1>,
    ),
    Stage6ReconciliationV2Error,
> {
    let transition = Stage6JournalRecordV2::decode_canonical(&transition.encode_canonical())?;
    let identity = transition.durable_request_identity().clone();
    let cancel_original_target_shape = if identity.action() == Stage6DurableActionKind::Cancel {
        let order = transition
            .payload()
            .broker_order_fact
            .as_ref()
            .ok_or(Stage6ReconciliationV2Error::InvalidSuffixManifest)?;
        Some(
            Stage6DurablePlaceOrderShapeV1::new(
                order.side,
                order.order_type,
                order.qty,
                order.limit_price,
                order
                    .time_in_force
                    .ok_or(Stage6ReconciliationV2Error::InvalidSuffixManifest)?,
            )
            .map_err(|_| Stage6ReconciliationV2Error::InvalidSuffixManifest)?,
        )
    } else {
        None
    };
    let Stage6ReconciliationTransitionKindV2::Exact { lifecycle } =
        *transition.payload().transition_kind()
    else {
        if transition.payload().suffix_manifest().entries().is_empty() {
            return Ok((Vec::new(), cancel_original_target_shape));
        }
        return Err(Stage6ReconciliationV2Error::InvalidSuffixManifest);
    };
    if identity.action() == Stage6DurableActionKind::Cancel
        && lifecycle == Stage6ReconciliationLifecycleV2::Working
    {
        if transition.payload().suffix_manifest().entries().is_empty() {
            return Ok((Vec::new(), cancel_original_target_shape));
        }
        return Err(Stage6ReconciliationV2Error::InvalidSuffixManifest);
    }

    let mut records = Vec::new();
    let mut previous = transition.journal_record_id().clone();
    let mut ordinal = 1_u64;
    let source = transition.source_evidence_sha256().clone();
    let next_sequence = |offset: u64| {
        transition
            .lifecycle_sequence()
            .get()
            .checked_add(offset)
            .and_then(|value| Stage6LifecycleSequence::new(value).ok())
            .ok_or(Stage6ReconciliationV2Error::InvalidSuffixManifest)
    };

    match identity.action() {
        Stage6DurableActionKind::Place => {
            let selected_order_id = transition
                .payload()
                .broker_order_fact()
                .and_then(Stage6BrokerOrderFactV2::broker_order_id)
                .cloned();
            let mut trades = transition
                .payload()
                .material_trade_facts()
                .iter()
                .collect::<Vec<_>>();
            trades.sort_by(|left, right| {
                left.broker_trade_id()
                    .as_str()
                    .cmp(right.broker_trade_id().as_str())
            });
            let material_order_ids = trades
                .iter()
                .filter_map(|trade| trade.broker_order_id())
                .map(BrokerOrderId::as_str)
                .collect::<BTreeSet<_>>();
            let projected_trade_order_id = match selected_order_id.as_ref() {
                Some(order_id) => {
                    if material_order_ids
                        .iter()
                        .any(|candidate| *candidate != order_id.as_str())
                    {
                        return Err(Stage6ReconciliationV2Error::InvalidSuffixManifest);
                    }
                    Some(order_id.clone())
                }
                None => {
                    if material_order_ids.len() > 1 {
                        return Err(Stage6ReconciliationV2Error::InvalidSuffixManifest);
                    }
                    trades
                        .iter()
                        .find_map(|trade| trade.broker_order_id().cloned())
                }
            };
            if let Some(order_id) = selected_order_id {
                let record = Stage6JournalRecordV1::broker_order_observed(
                    identity.clone(),
                    order_id,
                    next_sequence(ordinal)?,
                    Some(previous.clone()),
                    source.clone(),
                )
                .map_err(|_| Stage6ReconciliationV2Error::InvalidSuffixManifest)?;
                previous = record.journal_record_id().clone();
                ordinal += 1;
                records.push(record);
            }
            if let Some(order_id) = projected_trade_order_id {
                for trade in trades {
                    if trade.broker_order_id() != Some(&order_id) {
                        continue;
                    }
                    let record = Stage6JournalRecordV1::broker_trade_observed(
                        identity.clone(),
                        trade.broker_trade_id().clone(),
                        order_id.clone(),
                        next_sequence(ordinal)?,
                        Some(previous.clone()),
                        source.clone(),
                    )
                    .map_err(|_| Stage6ReconciliationV2Error::InvalidSuffixManifest)?;
                    previous = record.journal_record_id().clone();
                    ordinal += 1;
                    records.push(record);
                }
            }
            let disposition = if lifecycle == Stage6ReconciliationLifecycleV2::TerminalRejected {
                Stage6RequestFinalDispositionV1::Rejected
            } else {
                Stage6RequestFinalDispositionV1::Completed
            };
            records.push(
                Stage6JournalRecordV1::request_finalized(
                    identity.clone(),
                    disposition,
                    next_sequence(ordinal)?,
                    Some(previous),
                    source,
                )
                .map_err(|_| Stage6ReconciliationV2Error::InvalidSuffixManifest)?,
            );
        }
        Stage6DurableActionKind::Cancel => {
            let target = identity
                .target_broker_order_id()
                .cloned()
                .ok_or(Stage6ReconciliationV2Error::InvalidSuffixManifest)?;
            let outcome = match lifecycle {
                Stage6ReconciliationLifecycleV2::TerminalFilled => {
                    Stage6CancelOutcomeV1::ExecutionObserved
                }
                Stage6ReconciliationLifecycleV2::TerminalCancelled => {
                    Stage6CancelOutcomeV1::Canceled
                }
                Stage6ReconciliationLifecycleV2::TerminalRejected
                | Stage6ReconciliationLifecycleV2::TerminalExpired => {
                    Stage6CancelOutcomeV1::AlreadyTerminalNonExecution
                }
                Stage6ReconciliationLifecycleV2::Working => {
                    return Err(Stage6ReconciliationV2Error::InvalidSuffixManifest)
                }
            };
            let record = Stage6JournalRecordV1::cancel_outcome_observed(
                identity.clone(),
                target,
                outcome,
                next_sequence(ordinal)?,
                Some(previous.clone()),
                source.clone(),
            )
            .map_err(|_| Stage6ReconciliationV2Error::InvalidSuffixManifest)?;
            previous = record.journal_record_id().clone();
            ordinal += 1;
            records.push(record);
            records.push(
                Stage6JournalRecordV1::request_finalized(
                    identity,
                    Stage6RequestFinalDispositionV1::Completed,
                    next_sequence(ordinal)?,
                    Some(previous),
                    source,
                )
                .map_err(|_| Stage6ReconciliationV2Error::InvalidSuffixManifest)?,
            );
        }
    }
    let manifest = transition.payload().suffix_manifest().entries();
    records.truncate(manifest.len());
    if manifest.len() != records.len()
        || manifest
            .iter()
            .zip(&records)
            .any(|(entry, record)| !entry.matches_record(record))
    {
        return Err(Stage6ReconciliationV2Error::InvalidSuffixManifest);
    }
    Ok((records, cancel_original_target_shape))
}

/// Stage 8B-P1-d3 atomic outcome envelope.  The complete canonical outcome
/// evidence is carried in this Stage 6 frame; the digest is an integrity
/// binding, never a replacement sidecar.  Construction remains crate-private
/// so decoded material cannot mint journal-write authority.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stage6JournalRecordV3 {
    schema_version: u16,
    journal_record_id: Stage6JournalRecordId,
    lifecycle_sequence: Stage6LifecycleSequence,
    previous_record_id: Stage6JournalRecordId,
    event_kind: Stage6JournalEventKindV3,
    operational_identity_sha256: String,
    outcome_kind: String,
    transition_ordinal: u64,
    outcome_evidence_bytes: Vec<u8>,
    outcome_evidence_sha256: Stage6Sha256Digest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage6JournalEventKindV3 {
    P1d3OutcomeRecorded,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage6JournalRecordWireV3 {
    schema_version: u16,
    journal_record_id: Stage6JournalRecordId,
    lifecycle_sequence: Stage6LifecycleSequence,
    previous_record_id: Stage6JournalRecordId,
    event_kind: Stage6JournalEventKindV3,
    operational_identity_sha256: String,
    outcome_kind: String,
    transition_ordinal: u64,
    outcome_evidence_bytes: Vec<u8>,
    outcome_evidence_sha256: Stage6Sha256Digest,
}

impl From<Stage6JournalRecordWireV3> for Stage6JournalRecordV3 {
    fn from(value: Stage6JournalRecordWireV3) -> Self {
        Self {
            schema_version: value.schema_version,
            journal_record_id: value.journal_record_id,
            lifecycle_sequence: value.lifecycle_sequence,
            previous_record_id: value.previous_record_id,
            event_kind: value.event_kind,
            operational_identity_sha256: value.operational_identity_sha256,
            outcome_kind: value.outcome_kind,
            transition_ordinal: value.transition_ordinal,
            outcome_evidence_bytes: value.outcome_evidence_bytes,
            outcome_evidence_sha256: value.outcome_evidence_sha256,
        }
    }
}

impl Stage6JournalRecordV3 {
    pub(crate) fn from_p1d3_outcome_evidence(
        lifecycle_sequence: Stage6LifecycleSequence,
        previous_record_id: Stage6JournalRecordId,
        outcome_evidence_bytes: Vec<u8>,
    ) -> Result<Self, Stage6ReconciliationV2Error> {
        let evidence =
            crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeEvidenceV1::decode_canonical(
                &outcome_evidence_bytes,
            )
            .map_err(|_| Stage6ReconciliationV2Error::InvalidPayload)?;
        let journal_record_id =
            Stage6JournalRecordId::parse_exact(evidence.stage6_outcome_record_id().to_string())
                .map_err(|_| Stage6ReconciliationV2Error::RecordIdentityMismatch)?;
        let outcome_evidence_sha256 = Stage6Sha256Digest::parse(
            evidence
                .digest_sha256()
                .map_err(|_| Stage6ReconciliationV2Error::InvalidPayload)?,
        )
        .map_err(|_| Stage6ReconciliationV2Error::PayloadDigestMismatch)?;
        let value = Self {
            schema_version: STAGE6_DURABLE_RECORD_SCHEMA_VERSION_V3,
            journal_record_id,
            lifecycle_sequence,
            previous_record_id,
            event_kind: Stage6JournalEventKindV3::P1d3OutcomeRecorded,
            operational_identity_sha256: evidence.operational_identity_sha256().to_string(),
            outcome_kind: evidence.outcome_kind().canonical_name().to_string(),
            transition_ordinal: evidence.transition_ordinal(),
            outcome_evidence_bytes,
            outcome_evidence_sha256,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn encode_canonical(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("fixed V3 record serializes")
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Stage6ReconciliationV2Error> {
        let wire: Stage6JournalRecordWireV3 =
            serde_json::from_slice(bytes).map_err(|_| Stage6ReconciliationV2Error::DecodeFailed)?;
        let value = Self::from(wire);
        value.validate()?;
        if value.encode_canonical() != bytes {
            return Err(Stage6ReconciliationV2Error::NonCanonicalEncoding);
        }
        Ok(value)
    }

    pub fn journal_record_id(&self) -> &Stage6JournalRecordId {
        &self.journal_record_id
    }

    pub fn lifecycle_sequence(&self) -> Stage6LifecycleSequence {
        self.lifecycle_sequence
    }

    pub fn previous_record_id(&self) -> &Stage6JournalRecordId {
        &self.previous_record_id
    }

    pub fn outcome_evidence_bytes(&self) -> &[u8] {
        &self.outcome_evidence_bytes
    }

    pub fn outcome_evidence_sha256(&self) -> &Stage6Sha256Digest {
        &self.outcome_evidence_sha256
    }

    pub fn operational_identity_sha256(&self) -> &str {
        &self.operational_identity_sha256
    }

    pub(crate) fn authenticate_p1d3_outcome(
        &self,
        binding: crate::stage8b_p1d3_working_limit::Stage8bP1d3Stage6RecoveryBinding,
    ) -> Result<
        crate::stage8b_p1d3_working_limit::Stage8bP1d3AuthenticatedOutcomeEvidence,
        crate::stage8b_p1d3_working_limit::Stage8bP1d3Error,
    > {
        if binding.outcome_record_id != self.journal_record_id.as_str() {
            return Err(crate::stage8b_p1d3_working_limit::Stage8bP1d3Error::IdentityMismatch);
        }
        crate::stage8b_p1d3_working_limit::authenticate_stage8b_p1d3_outcome_evidence(
            self.outcome_evidence_bytes.clone(),
            binding,
        )
    }

    fn transition_key(&self) -> String {
        format!(
            "{}\0{}\0{}",
            self.operational_identity_sha256, self.outcome_kind, self.transition_ordinal
        )
    }

    fn validate(&self) -> Result<(), Stage6ReconciliationV2Error> {
        if self.schema_version != STAGE6_DURABLE_RECORD_SCHEMA_VERSION_V3
            || self.event_kind != Stage6JournalEventKindV3::P1d3OutcomeRecorded
            || self.transition_ordinal == 0
        {
            return Err(Stage6ReconciliationV2Error::InvalidPayload);
        }
        let evidence =
            crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeEvidenceV1::decode_canonical(
                &self.outcome_evidence_bytes,
            )
            .map_err(|_| Stage6ReconciliationV2Error::InvalidPayload)?;
        let expected_id = Stage6JournalRecordId::derive_stage8b_p1d3_outcome(
            evidence.operational_identity_sha256(),
            evidence.broker_order_id(),
            evidence.transition_ordinal(),
            evidence.outcome_kind().canonical_name(),
        );
        let expected_digest = Stage6Sha256Digest::parse(
            evidence
                .digest_sha256()
                .map_err(|_| Stage6ReconciliationV2Error::InvalidPayload)?,
        )
        .map_err(|_| Stage6ReconciliationV2Error::PayloadDigestMismatch)?;
        if self.journal_record_id != expected_id
            || self.journal_record_id.as_str() != evidence.stage6_outcome_record_id()
            || self.operational_identity_sha256 != evidence.operational_identity_sha256()
            || self.outcome_kind != evidence.outcome_kind().canonical_name()
            || self.transition_ordinal != evidence.transition_ordinal()
            || self.outcome_evidence_sha256 != expected_digest
        {
            return Err(Stage6ReconciliationV2Error::RecordIdentityMismatch);
        }
        Ok(())
    }
}

/// Canonical M10 identity embedded in a Stage 6 V4 schedule binding. The
/// timestamp strings intentionally follow the reviewed wire schema rather
/// than inheriting any process-local chrono representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage6ScheduleM10IdentityV4 {
    redis_id: String,
    semantic_id_sha256: String,
    payload_sha256: String,
    open_ts_utc: String,
    close_ts_utc: String,
}

impl Stage6ScheduleM10IdentityV4 {
    fn from_candidate(
        value: &crate::Stage8bP1eM10IdentityV1,
    ) -> Result<Self, Stage6ReconciliationV2Error> {
        Ok(Self {
            redis_id: value.redis_id.clone(),
            semantic_id_sha256: value.semantic_id_sha256.clone(),
            payload_sha256: value.payload_sha256.clone(),
            open_ts_utc: value
                .open_ts_utc()
                .ok_or(Stage6ReconciliationV2Error::InvalidPayload)?,
            close_ts_utc: value
                .close_ts_utc()
                .ok_or(Stage6ReconciliationV2Error::InvalidPayload)?,
        })
    }
}

/// Stage 8B-P1-e I1A durable binding. It consumes one global Stage 6 sequence
/// but is not a business terminal boundary and has no source-XACK semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage6JournalRecordV4 {
    schema_version: u16,
    record_kind: String,
    journal_record_id: Stage6JournalRecordId,
    previous_record_id: Stage6JournalRecordId,
    causal_parent_id: Stage6JournalRecordId,
    lifecycle_sequence: String,
    prior_covering_seal_generation: String,
    expected_covering_seal_generation: String,
    operational_identity_sha256: String,
    transition_kind: crate::Stage8bP1eScheduleTransitionKindV1,
    transition_binding_sha256: String,
    authority_kind: crate::Stage8bP1eScheduleAuthorityKindV1,
    trading_day: String,
    instrument: String,
    timeframe_sec: u32,
    predecessor_m10: Stage6ScheduleM10IdentityV4,
    candidate_or_last_eligible_m10: Stage6ScheduleM10IdentityV4,
    request_or_order_binding: crate::Stage8bP1eRequestOrOrderBindingV1,
    redis_stream_id: String,
    source_generation: String,
    publication_sequence: String,
    semantic_revision: String,
    schedule_semantic_sha256: String,
    exact_envelope_hex: String,
    envelope_sha256: String,
    bound_at_utc: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage6JournalRecordWireV4 {
    schema_version: u16,
    record_kind: String,
    journal_record_id: Stage6JournalRecordId,
    previous_record_id: Stage6JournalRecordId,
    causal_parent_id: Stage6JournalRecordId,
    lifecycle_sequence: String,
    prior_covering_seal_generation: String,
    expected_covering_seal_generation: String,
    operational_identity_sha256: String,
    transition_kind: crate::Stage8bP1eScheduleTransitionKindV1,
    transition_binding_sha256: String,
    authority_kind: crate::Stage8bP1eScheduleAuthorityKindV1,
    trading_day: String,
    instrument: String,
    timeframe_sec: u32,
    predecessor_m10: Stage6ScheduleM10IdentityV4,
    candidate_or_last_eligible_m10: Stage6ScheduleM10IdentityV4,
    request_or_order_binding: crate::Stage8bP1eRequestOrOrderBindingV1,
    redis_stream_id: String,
    source_generation: String,
    publication_sequence: String,
    semantic_revision: String,
    schedule_semantic_sha256: String,
    exact_envelope_hex: String,
    envelope_sha256: String,
    bound_at_utc: String,
}

impl From<Stage6JournalRecordWireV4> for Stage6JournalRecordV4 {
    fn from(value: Stage6JournalRecordWireV4) -> Self {
        Self {
            schema_version: value.schema_version,
            record_kind: value.record_kind,
            journal_record_id: value.journal_record_id,
            previous_record_id: value.previous_record_id,
            causal_parent_id: value.causal_parent_id,
            lifecycle_sequence: value.lifecycle_sequence,
            prior_covering_seal_generation: value.prior_covering_seal_generation,
            expected_covering_seal_generation: value.expected_covering_seal_generation,
            operational_identity_sha256: value.operational_identity_sha256,
            transition_kind: value.transition_kind,
            transition_binding_sha256: value.transition_binding_sha256,
            authority_kind: value.authority_kind,
            trading_day: value.trading_day,
            instrument: value.instrument,
            timeframe_sec: value.timeframe_sec,
            predecessor_m10: value.predecessor_m10,
            candidate_or_last_eligible_m10: value.candidate_or_last_eligible_m10,
            request_or_order_binding: value.request_or_order_binding,
            redis_stream_id: value.redis_stream_id,
            source_generation: value.source_generation,
            publication_sequence: value.publication_sequence,
            semantic_revision: value.semantic_revision,
            schedule_semantic_sha256: value.schedule_semantic_sha256,
            exact_envelope_hex: value.exact_envelope_hex,
            envelope_sha256: value.envelope_sha256,
            bound_at_utc: value.bound_at_utc,
        }
    }
}

#[derive(Serialize)]
struct Stage6JournalRecordBodyV4<'a> {
    schema_version: u16,
    record_kind: &'a str,
    previous_record_id: &'a Stage6JournalRecordId,
    causal_parent_id: &'a Stage6JournalRecordId,
    lifecycle_sequence: &'a str,
    prior_covering_seal_generation: &'a str,
    expected_covering_seal_generation: &'a str,
    operational_identity_sha256: &'a str,
    transition_kind: crate::Stage8bP1eScheduleTransitionKindV1,
    transition_binding_sha256: &'a str,
    authority_kind: crate::Stage8bP1eScheduleAuthorityKindV1,
    trading_day: &'a str,
    instrument: &'a str,
    timeframe_sec: u32,
    predecessor_m10: &'a Stage6ScheduleM10IdentityV4,
    candidate_or_last_eligible_m10: &'a Stage6ScheduleM10IdentityV4,
    request_or_order_binding: &'a crate::Stage8bP1eRequestOrOrderBindingV1,
    redis_stream_id: &'a str,
    source_generation: &'a str,
    publication_sequence: &'a str,
    semantic_revision: &'a str,
    schedule_semantic_sha256: &'a str,
    exact_envelope_hex: &'a str,
    envelope_sha256: &'a str,
    bound_at_utc: &'a str,
}

#[derive(Serialize)]
struct Stage6ScheduleTransitionBindingV4<'a> {
    operational_identity_sha256: &'a str,
    transition_kind: crate::Stage8bP1eScheduleTransitionKindV1,
    authority_kind: crate::Stage8bP1eScheduleAuthorityKindV1,
    trading_day: &'a str,
    instrument: &'a str,
    timeframe_sec: u32,
    predecessor_m10: &'a Stage6ScheduleM10IdentityV4,
    candidate_or_last_eligible_m10: &'a Stage6ScheduleM10IdentityV4,
    request_or_order_binding: &'a crate::Stage8bP1eRequestOrOrderBindingV1,
    redis_stream_id: &'a str,
    source_generation: &'a str,
    publication_sequence: &'a str,
    semantic_revision: &'a str,
    schedule_semantic_sha256: &'a str,
    envelope_sha256: &'a str,
}

impl Stage6JournalRecordV4 {
    pub(crate) fn from_stage8b_p1e_candidate(
        candidate: &crate::Stage8bP1eScheduleBindingCandidateV1,
        lifecycle_sequence: Stage6LifecycleSequence,
        previous_record_id: Stage6JournalRecordId,
        prior_covering_seal_generation: u64,
        bound_at_utc: DateTime<Utc>,
    ) -> Result<Self, Stage6ReconciliationV2Error> {
        let expected_covering_seal_generation = prior_covering_seal_generation
            .checked_add(1)
            .ok_or(Stage6ReconciliationV2Error::InvalidPayload)?;
        let predecessor_m10 =
            Stage6ScheduleM10IdentityV4::from_candidate(candidate.predecessor_m10())?;
        let candidate_or_last_eligible_m10 = Stage6ScheduleM10IdentityV4::from_candidate(
            candidate.candidate_or_last_eligible_m10(),
        )?;
        let exact_envelope_hex = encode_lower_hex(candidate.exact_envelope_bytes());
        let mut value = Self {
            schema_version: STAGE6_DURABLE_RECORD_SCHEMA_VERSION_V4,
            record_kind: "schedule_evidence_bound".to_string(),
            journal_record_id: Stage6JournalRecordId::parse_exact("1".repeat(64))
                .map_err(|_| Stage6ReconciliationV2Error::RecordIdentityMismatch)?,
            previous_record_id: previous_record_id.clone(),
            causal_parent_id: previous_record_id,
            lifecycle_sequence: lifecycle_sequence.get().to_string(),
            prior_covering_seal_generation: prior_covering_seal_generation.to_string(),
            expected_covering_seal_generation: expected_covering_seal_generation.to_string(),
            operational_identity_sha256: candidate.operational_identity_sha256().to_string(),
            transition_kind: candidate.transition_kind(),
            transition_binding_sha256: "1".repeat(64),
            authority_kind: candidate.authority_kind(),
            trading_day: candidate.trading_day().to_string(),
            instrument: "IMOEXF@RTSX".to_string(),
            timeframe_sec: 600,
            predecessor_m10,
            candidate_or_last_eligible_m10,
            request_or_order_binding: candidate.request_or_order_binding().clone(),
            redis_stream_id: candidate.redis_stream_id().to_string(),
            source_generation: candidate.source_generation().to_string(),
            publication_sequence: candidate.publication_sequence().to_string(),
            semantic_revision: candidate.semantic_revision().to_string(),
            schedule_semantic_sha256: candidate.schedule_semantic_sha256().to_string(),
            exact_envelope_hex,
            envelope_sha256: candidate.envelope_sha256().to_string(),
            bound_at_utc: bound_at_utc.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        };
        value.transition_binding_sha256 = value.derive_transition_binding_sha256()?;
        value.journal_record_id = value.derive_record_id()?;
        value.validate()?;
        if bound_at_utc > candidate.valid_until() || !value.matches_stage8b_p1e_candidate(candidate)
        {
            return Err(Stage6ReconciliationV2Error::InvalidPayload);
        }
        Ok(value)
    }

    pub fn encode_canonical(&self) -> Vec<u8> {
        canonical_json(self).expect("validated V4 record serializes")
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Stage6ReconciliationV2Error> {
        let wire: Stage6JournalRecordWireV4 =
            serde_json::from_slice(bytes).map_err(|_| Stage6ReconciliationV2Error::DecodeFailed)?;
        let value = Self::from(wire);
        value.validate()?;
        if value.encode_canonical() != bytes {
            return Err(Stage6ReconciliationV2Error::NonCanonicalEncoding);
        }
        Ok(value)
    }

    pub fn journal_record_id(&self) -> &Stage6JournalRecordId {
        &self.journal_record_id
    }

    pub fn lifecycle_sequence(&self) -> Stage6LifecycleSequence {
        Stage6LifecycleSequence::new(
            self.lifecycle_sequence
                .parse()
                .expect("validated V4 lifecycle sequence"),
        )
        .expect("validated nonzero V4 lifecycle sequence")
    }

    pub fn previous_record_id(&self) -> &Stage6JournalRecordId {
        &self.previous_record_id
    }

    pub fn prior_covering_seal_generation(&self) -> u64 {
        self.prior_covering_seal_generation
            .parse()
            .expect("validated V4 prior covering seal generation")
    }

    pub fn expected_covering_seal_generation(&self) -> u64 {
        self.expected_covering_seal_generation
            .parse()
            .expect("validated V4 covering seal generation")
    }

    pub fn envelope_sha256(&self) -> &str {
        &self.envelope_sha256
    }

    pub fn transition_binding_sha256(&self) -> &str {
        &self.transition_binding_sha256
    }

    pub fn operational_identity_sha256(&self) -> &str {
        &self.operational_identity_sha256
    }

    pub(crate) fn transition_kind(&self) -> crate::Stage8bP1eScheduleTransitionKindV1 {
        self.transition_kind
    }

    pub(crate) fn request_or_order_binding(&self) -> &crate::Stage8bP1eRequestOrOrderBindingV1 {
        &self.request_or_order_binding
    }

    pub(crate) fn redis_stream_id(&self) -> &str {
        &self.redis_stream_id
    }

    pub(crate) fn exact_envelope_bytes(&self) -> Result<Vec<u8>, Stage6ReconciliationV2Error> {
        decode_lower_hex_v4(&self.exact_envelope_hex)
    }

    pub(crate) fn predecessor_m10(&self) -> crate::Stage8bP1eM10IdentityV1 {
        self.predecessor_m10.to_p1e_identity()
    }

    pub(crate) fn candidate_or_last_eligible_m10(&self) -> crate::Stage8bP1eM10IdentityV1 {
        self.candidate_or_last_eligible_m10.to_p1e_identity()
    }

    pub(crate) fn bound_at_utc(&self) -> DateTime<Utc> {
        parse_exact_timestamp_v4(&self.bound_at_utc).expect("validated V4 bound timestamp")
    }

    pub(crate) fn matches_stage8b_p1e_candidate(
        &self,
        candidate: &crate::Stage8bP1eScheduleBindingCandidateV1,
    ) -> bool {
        let Ok(predecessor) =
            Stage6ScheduleM10IdentityV4::from_candidate(candidate.predecessor_m10())
        else {
            return false;
        };
        let Ok(current) =
            Stage6ScheduleM10IdentityV4::from_candidate(candidate.candidate_or_last_eligible_m10())
        else {
            return false;
        };
        self.operational_identity_sha256 == candidate.operational_identity_sha256()
            && self.transition_kind == candidate.transition_kind()
            && self.authority_kind == candidate.authority_kind()
            && self.trading_day == candidate.trading_day()
            && self.predecessor_m10 == predecessor
            && self.candidate_or_last_eligible_m10 == current
            && self.request_or_order_binding == *candidate.request_or_order_binding()
            && self.redis_stream_id == candidate.redis_stream_id()
            && self.source_generation == candidate.source_generation().to_string()
            && self.publication_sequence == candidate.publication_sequence().to_string()
            && self.semantic_revision == candidate.semantic_revision().to_string()
            && self.schedule_semantic_sha256 == candidate.schedule_semantic_sha256()
            && self.exact_envelope_hex == encode_lower_hex(candidate.exact_envelope_bytes())
            && self.envelope_sha256 == candidate.envelope_sha256()
    }

    fn transition_key(&self) -> &str {
        &self.transition_binding_sha256
    }

    fn body(&self) -> Stage6JournalRecordBodyV4<'_> {
        Stage6JournalRecordBodyV4 {
            schema_version: self.schema_version,
            record_kind: &self.record_kind,
            previous_record_id: &self.previous_record_id,
            causal_parent_id: &self.causal_parent_id,
            lifecycle_sequence: &self.lifecycle_sequence,
            prior_covering_seal_generation: &self.prior_covering_seal_generation,
            expected_covering_seal_generation: &self.expected_covering_seal_generation,
            operational_identity_sha256: &self.operational_identity_sha256,
            transition_kind: self.transition_kind,
            transition_binding_sha256: &self.transition_binding_sha256,
            authority_kind: self.authority_kind,
            trading_day: &self.trading_day,
            instrument: &self.instrument,
            timeframe_sec: self.timeframe_sec,
            predecessor_m10: &self.predecessor_m10,
            candidate_or_last_eligible_m10: &self.candidate_or_last_eligible_m10,
            request_or_order_binding: &self.request_or_order_binding,
            redis_stream_id: &self.redis_stream_id,
            source_generation: &self.source_generation,
            publication_sequence: &self.publication_sequence,
            semantic_revision: &self.semantic_revision,
            schedule_semantic_sha256: &self.schedule_semantic_sha256,
            exact_envelope_hex: &self.exact_envelope_hex,
            envelope_sha256: &self.envelope_sha256,
            bound_at_utc: &self.bound_at_utc,
        }
    }

    fn transition_binding(&self) -> Stage6ScheduleTransitionBindingV4<'_> {
        Stage6ScheduleTransitionBindingV4 {
            operational_identity_sha256: &self.operational_identity_sha256,
            transition_kind: self.transition_kind,
            authority_kind: self.authority_kind,
            trading_day: &self.trading_day,
            instrument: &self.instrument,
            timeframe_sec: self.timeframe_sec,
            predecessor_m10: &self.predecessor_m10,
            candidate_or_last_eligible_m10: &self.candidate_or_last_eligible_m10,
            request_or_order_binding: &self.request_or_order_binding,
            redis_stream_id: &self.redis_stream_id,
            source_generation: &self.source_generation,
            publication_sequence: &self.publication_sequence,
            semantic_revision: &self.semantic_revision,
            schedule_semantic_sha256: &self.schedule_semantic_sha256,
            envelope_sha256: &self.envelope_sha256,
        }
    }

    fn derive_record_id(&self) -> Result<Stage6JournalRecordId, Stage6ReconciliationV2Error> {
        let mut hasher = sha2::Sha256::new();
        hasher.update(b"moex.stage6.schedule-evidence-bound.record-id.v1");
        hasher.update(b"\0");
        hasher.update(canonical_json(&self.body())?);
        Stage6JournalRecordId::parse_exact(format!("{:x}", hasher.finalize()))
            .map_err(|_| Stage6ReconciliationV2Error::RecordIdentityMismatch)
    }

    fn derive_transition_binding_sha256(&self) -> Result<String, Stage6ReconciliationV2Error> {
        let mut hasher = sha2::Sha256::new();
        hasher.update(b"moex.stage8b.p1e.schedule-transition-binding.sha256.v1");
        hasher.update(b"\0");
        hasher.update(canonical_json(&self.transition_binding())?);
        Ok(format!("{:x}", hasher.finalize()))
    }

    fn validate(&self) -> Result<(), Stage6ReconciliationV2Error> {
        let sequence = parse_nonzero_decimal(&self.lifecycle_sequence)?;
        let prior_generation = parse_decimal(&self.prior_covering_seal_generation)?;
        let expected_generation = parse_nonzero_decimal(&self.expected_covering_seal_generation)?;
        let source_generation = parse_nonzero_decimal(&self.source_generation)?;
        let publication_sequence = parse_nonzero_decimal(&self.publication_sequence)?;
        let semantic_revision = parse_nonzero_decimal(&self.semantic_revision)?;
        if self.schema_version != STAGE6_DURABLE_RECORD_SCHEMA_VERSION_V4
            || self.record_kind != "schedule_evidence_bound"
            || self.previous_record_id != self.causal_parent_id
            || expected_generation
                != prior_generation
                    .checked_add(1)
                    .ok_or(Stage6ReconciliationV2Error::InvalidPayload)?
            || sequence == 0
            || source_generation == 0
            || publication_sequence == 0
            || semantic_revision == 0
            || self.instrument != "IMOEXF@RTSX"
            || self.timeframe_sec != 600
            || !valid_sha256_text(&self.operational_identity_sha256)
            || !valid_sha256_text(&self.transition_binding_sha256)
            || !valid_sha256_text(&self.schedule_semantic_sha256)
            || !valid_sha256_text(&self.envelope_sha256)
            || !valid_redis_id(&self.redis_stream_id)
            || !valid_exact_timestamp(&self.bound_at_utc)
            || self.exact_envelope_hex.is_empty()
            || self.exact_envelope_hex.len() > 524_288
            || self.exact_envelope_hex.len() % 2 != 0
            || !self.exact_envelope_hex.bytes().all(is_lower_hex)
            || self.derive_transition_binding_sha256()? != self.transition_binding_sha256
            || self.derive_record_id()? != self.journal_record_id
        {
            return Err(Stage6ReconciliationV2Error::InvalidPayload);
        }
        validate_v4_m10(&self.predecessor_m10, &self.trading_day)?;
        validate_v4_m10(&self.candidate_or_last_eligible_m10, &self.trading_day)?;
        let predecessor_close = parse_exact_timestamp_v4(&self.predecessor_m10.close_ts_utc)?;
        let candidate_close =
            parse_exact_timestamp_v4(&self.candidate_or_last_eligible_m10.close_ts_utc)?;
        if candidate_close <= predecessor_close
            || sha256_hex_v4(&decode_lower_hex_v4(&self.exact_envelope_hex)?)
                != self.envelope_sha256
            || !valid_v4_route_binding(
                self.transition_kind,
                self.authority_kind,
                &self.request_or_order_binding,
            )
        {
            return Err(Stage6ReconciliationV2Error::InvalidPayload);
        }
        Ok(())
    }
}

impl Stage6ScheduleM10IdentityV4 {
    fn to_p1e_identity(&self) -> crate::Stage8bP1eM10IdentityV1 {
        crate::Stage8bP1eM10IdentityV1 {
            close_ts_utc_ms: parse_exact_timestamp_v4(&self.close_ts_utc)
                .expect("validated V4 close timestamp")
                .timestamp_millis(),
            open_ts_utc_ms: parse_exact_timestamp_v4(&self.open_ts_utc)
                .expect("validated V4 open timestamp")
                .timestamp_millis(),
            payload_sha256: self.payload_sha256.clone(),
            redis_id: self.redis_id.clone(),
            semantic_id_sha256: self.semantic_id_sha256.clone(),
        }
    }
}

fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, Stage6ReconciliationV2Error> {
    let value =
        serde_json::to_value(value).map_err(|_| Stage6ReconciliationV2Error::DecodeFailed)?;
    serde_json::to_vec(&value).map_err(|_| Stage6ReconciliationV2Error::DecodeFailed)
}

fn parse_decimal(value: &str) -> Result<u64, Stage6ReconciliationV2Error> {
    if value == "0" {
        return Ok(0);
    }
    parse_nonzero_decimal(value)
}

fn parse_nonzero_decimal(value: &str) -> Result<u64, Stage6ReconciliationV2Error> {
    if value.is_empty()
        || value.len() > 20
        || value.as_bytes()[0] == b'0'
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(Stage6ReconciliationV2Error::InvalidPayload);
    }
    value
        .parse()
        .map_err(|_| Stage6ReconciliationV2Error::InvalidPayload)
}

fn valid_sha256_text(value: &str) -> bool {
    value.len() == 64 && value != "0".repeat(64) && value.bytes().all(is_lower_hex)
}

fn is_lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}

fn valid_redis_id(value: &str) -> bool {
    let Some((milliseconds, sequence)) = value.split_once('-') else {
        return false;
    };
    !sequence.contains('-')
        && parse_nonzero_decimal(milliseconds).is_ok()
        && (sequence == "0" || parse_nonzero_decimal(sequence).is_ok())
}

fn parse_exact_timestamp_v4(value: &str) -> Result<DateTime<Utc>, Stage6ReconciliationV2Error> {
    let parsed = DateTime::parse_from_rfc3339(value)
        .map_err(|_| Stage6ReconciliationV2Error::InvalidPayload)?
        .with_timezone(&Utc);
    if parsed.to_rfc3339_opts(chrono::SecondsFormat::Micros, true) != value {
        return Err(Stage6ReconciliationV2Error::InvalidPayload);
    }
    Ok(parsed)
}

fn valid_exact_timestamp(value: &str) -> bool {
    parse_exact_timestamp_v4(value).is_ok()
}

fn validate_v4_m10(
    value: &Stage6ScheduleM10IdentityV4,
    trading_day: &str,
) -> Result<(), Stage6ReconciliationV2Error> {
    let open = parse_exact_timestamp_v4(&value.open_ts_utc)?;
    let close = parse_exact_timestamp_v4(&value.close_ts_utc)?;
    let day = NaiveDate::parse_from_str(trading_day, "%Y-%m-%d")
        .map_err(|_| Stage6ReconciliationV2Error::InvalidPayload)?;
    if close - open != chrono::Duration::seconds(600)
        || close.date_naive() != day
        || value.redis_id != format!("{}-0", close.timestamp_millis())
        || !valid_sha256_text(&value.semantic_id_sha256)
        || !valid_sha256_text(&value.payload_sha256)
    {
        return Err(Stage6ReconciliationV2Error::InvalidPayload);
    }
    Ok(())
}

fn valid_v4_route_binding(
    transition_kind: crate::Stage8bP1eScheduleTransitionKindV1,
    authority_kind: crate::Stage8bP1eScheduleAuthorityKindV1,
    binding: &crate::Stage8bP1eRequestOrOrderBindingV1,
) -> bool {
    match (transition_kind, authority_kind) {
        (
            crate::Stage8bP1eScheduleTransitionKindV1::MarketExecution,
            crate::Stage8bP1eScheduleAuthorityKindV1::Market,
        ) => {
            binding
                .strategy_request_id
                .as_deref()
                .is_some_and(|value| !value.is_empty() && value.len() <= 256)
                && binding
                    .canonical_command_sha256
                    .as_deref()
                    .is_some_and(valid_sha256_text)
                && binding.active_broker_order_id.is_none()
                && binding.working_book_transition_sha256.is_none()
        }
        (
            crate::Stage8bP1eScheduleTransitionKindV1::WorkingLimitEvaluation
            | crate::Stage8bP1eScheduleTransitionKindV1::CancelStep,
            crate::Stage8bP1eScheduleAuthorityKindV1::ScheduleStep,
        )
        | (
            crate::Stage8bP1eScheduleTransitionKindV1::DayExpiry,
            crate::Stage8bP1eScheduleAuthorityKindV1::DayExpiry,
        ) => {
            binding.strategy_request_id.is_none()
                && binding.canonical_command_sha256.is_none()
                && binding
                    .active_broker_order_id
                    .as_deref()
                    .is_some_and(|value| !value.is_empty() && value.len() <= 256)
                && binding
                    .working_book_transition_sha256
                    .as_deref()
                    .is_some_and(valid_sha256_text)
        }
        _ => false,
    }
}

fn encode_lower_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_lower_hex_v4(value: &str) -> Result<Vec<u8>, Stage6ReconciliationV2Error> {
    if value.is_empty() || value.len() % 2 != 0 || !value.bytes().all(is_lower_hex) {
        return Err(Stage6ReconciliationV2Error::InvalidPayload);
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|_| Stage6ReconciliationV2Error::InvalidPayload)
        })
        .collect()
}

fn sha256_hex_v4(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}

#[derive(Debug, Clone, PartialEq)]
// V1/V2/V3 remain byte-for-byte stable; P1-e adds one new read/replay variant.
// This read-only replay enum is not a high-volume queue element.
#[allow(clippy::large_enum_variant)]
pub enum Stage6JournalRecordVersioned {
    V1(Stage6JournalRecordV1),
    V2(Stage6JournalRecordV2),
    V3(Stage6JournalRecordV3),
    V4(Stage6JournalRecordV4),
}

impl Stage6JournalRecordVersioned {
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Stage6ReconciliationV2Error> {
        match probe_schema_version(bytes)? {
            1 => Stage6JournalRecordV1::decode_canonical(bytes)
                .map(Self::V1)
                .map_err(map_v1_error),
            2 => Stage6JournalRecordV2::decode_canonical(bytes).map(Self::V2),
            3 => Stage6JournalRecordV3::decode_canonical(bytes).map(Self::V3),
            4 => Stage6JournalRecordV4::decode_canonical(bytes).map(Self::V4),
            value => Err(Stage6ReconciliationV2Error::UnsupportedSchema(value)),
        }
    }
    pub fn encode_canonical(&self) -> Vec<u8> {
        match self {
            Self::V1(value) => value.encode_canonical(),
            Self::V2(value) => value.encode_canonical(),
            Self::V3(value) => value.encode_canonical(),
            Self::V4(value) => value.encode_canonical(),
        }
    }
    pub fn journal_record_id(&self) -> &Stage6JournalRecordId {
        match self {
            Self::V1(value) => value.journal_record_id(),
            Self::V2(value) => value.journal_record_id(),
            Self::V3(value) => value.journal_record_id(),
            Self::V4(value) => value.journal_record_id(),
        }
    }
    pub fn lifecycle_sequence(&self) -> Stage6LifecycleSequence {
        match self {
            Self::V1(value) => value.lifecycle_sequence(),
            Self::V2(value) => value.lifecycle_sequence(),
            Self::V3(value) => value.lifecycle_sequence(),
            Self::V4(value) => value.lifecycle_sequence(),
        }
    }
}

fn map_v1_error(error: Stage6DurableIdentityError) -> Stage6ReconciliationV2Error {
    match error {
        Stage6DurableIdentityError::UnsupportedSchema => {
            Stage6ReconciliationV2Error::UnsupportedSchema(1)
        }
        Stage6DurableIdentityError::NonCanonicalEncoding => {
            Stage6ReconciliationV2Error::NonCanonicalEncoding
        }
        _ => Stage6ReconciliationV2Error::DecodeFailed,
    }
}

struct SchemaProbeVisitor;
impl<'de> Visitor<'de> for SchemaProbeVisitor {
    type Value = u64;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a Stage 6 record object")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut schema = None;
        while let Some(key) = map.next_key::<String>()? {
            if key == "schema_version" {
                if schema.is_some() {
                    return Err(serde::de::Error::custom("duplicate schema_version"));
                }
                schema = Some(map.next_value::<u64>()?);
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        schema.ok_or_else(|| serde::de::Error::custom("missing schema_version"))
    }
}

fn probe_schema_version(bytes: &[u8]) -> Result<u64, Stage6ReconciliationV2Error> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = deserializer
        .deserialize_map(SchemaProbeVisitor)
        .map_err(|_| Stage6ReconciliationV2Error::AmbiguousSchema)?;
    deserializer
        .end()
        .map_err(|_| Stage6ReconciliationV2Error::DecodeFailed)?;
    Ok(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage6ReconciliationBatchCompletionV2 {
    Incomplete,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stage6PendingReconciliationBatchV2 {
    transition_record: Stage6JournalRecordV2,
    verified_suffix_prefix_length: usize,
    completion: Stage6ReconciliationBatchCompletionV2,
    last_mixed_record_id: Stage6JournalRecordId,
    last_mixed_lifecycle_sequence: Stage6LifecycleSequence,
}

impl Stage6PendingReconciliationBatchV2 {
    pub fn transition_record(&self) -> &Stage6JournalRecordV2 {
        &self.transition_record
    }
    pub fn verified_suffix_prefix_length(&self) -> usize {
        self.verified_suffix_prefix_length
    }
    pub fn completion(&self) -> Stage6ReconciliationBatchCompletionV2 {
        self.completion
    }
    pub fn stable_transition_key_sha256(&self) -> &Stage6Sha256Digest {
        self.transition_record
            .payload
            .stable_transition_key_sha256()
    }
    pub fn transition_kind(&self) -> &Stage6ReconciliationTransitionKindV2 {
        self.transition_record.payload.transition_kind()
    }
    pub fn canonical_v2_record_sha256(&self) -> Stage6Sha256Digest {
        self.transition_record.canonical_record_sha256()
    }
    pub fn suffix_manifest(&self) -> &Stage6SuffixManifestV2 {
        self.transition_record.payload.suffix_manifest()
    }
    pub fn last_mixed_record_id(&self) -> &Stage6JournalRecordId {
        &self.last_mixed_record_id
    }
    pub fn last_mixed_lifecycle_sequence(&self) -> Stage6LifecycleSequence {
        self.last_mixed_lifecycle_sequence
    }
    pub fn missing_suffix_entries(&self) -> &[Stage6SuffixManifestEntryV2] {
        &self
            .transition_record
            .payload
            .deterministic_suffix_manifest
            .entries[self.verified_suffix_prefix_length..]
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stage6MixedReplaySnapshotV2 {
    requests: Vec<Stage6RecoveredRequestV1>,
    reconciliation_batches: Vec<Stage6PendingReconciliationBatchV2>,
    p1d3_outcome_records: Vec<Stage6JournalRecordV3>,
    schedule_binding_records: Vec<Stage6JournalRecordV4>,
}

impl Stage6MixedReplaySnapshotV2 {
    pub fn requests(&self) -> &[Stage6RecoveredRequestV1] {
        &self.requests
    }
    pub fn reconciliation_batches(&self) -> &[Stage6PendingReconciliationBatchV2] {
        &self.reconciliation_batches
    }

    pub fn p1d3_outcome_records(&self) -> &[Stage6JournalRecordV3] {
        &self.p1d3_outcome_records
    }

    pub fn schedule_binding_records(&self) -> &[Stage6JournalRecordV4] {
        &self.schedule_binding_records
    }

    pub(crate) fn into_requests(self) -> Vec<Stage6RecoveredRequestV1> {
        self.requests
    }
}

#[derive(Debug, Default)]
pub struct Stage6MixedReplayEngineV2;

struct MixedWorkingRequest {
    v1: WorkingRequest,
    batch: Option<Stage6PendingReconciliationBatchV2>,
}

impl Stage6MixedReplayEngineV2 {
    pub fn replay(
        records: &[Stage6JournalRecordVersioned],
    ) -> Result<Stage6MixedReplaySnapshotV2, Stage6ReconciliationV2Error> {
        let mut seen = BTreeMap::<String, Vec<u8>>::new();
        let mut seen_transition_keys = BTreeMap::<String, Vec<u8>>::new();
        let mut seen_p1d3_transition_keys = BTreeMap::<String, Vec<u8>>::new();
        let mut seen_schedule_transition_keys = BTreeMap::<String, Vec<u8>>::new();
        let mut requests = BTreeMap::<String, MixedWorkingRequest>::new();
        let mut p1d3_outcome_records = Vec::new();
        let mut schedule_binding_records = Vec::new();
        let mut last_unique_record_id: Option<Stage6JournalRecordId> = None;
        let mut last_unique_lifecycle_sequence: Option<Stage6LifecycleSequence> = None;
        let mut last_unique_request_accepted_identity: Option<Stage6DurableRequestIdentityV1> =
            None;
        for record in records {
            let key = record.journal_record_id().as_str().to_string();
            let canonical = record.encode_canonical();
            if let Some(existing) = seen.get(&key) {
                if existing == &canonical {
                    continue;
                }
                return Err(Stage6ReconciliationV2Error::Replay(
                    Stage6ReplayError::ConflictingReplay,
                ));
            }
            match record {
                Stage6JournalRecordVersioned::V1(v1) => {
                    if v1
                        .causal_parent_id()
                        .is_some_and(|parent| !seen.contains_key(parent.as_str()))
                    {
                        return Err(Stage6ReconciliationV2Error::Replay(
                            Stage6ReplayError::CausalParentMissing,
                        ));
                    }
                    let request_key = v1
                        .durable_request_identity()
                        .strategy_request_id()
                        .to_string();
                    match requests.get_mut(&request_key) {
                        None => {
                            requests.insert(
                                request_key,
                                MixedWorkingRequest {
                                    v1: WorkingRequest::from_first(v1)?,
                                    batch: None,
                                },
                            );
                        }
                        Some(state) => {
                            if let Some(batch) = state.batch.as_mut() {
                                if batch.completion
                                    == Stage6ReconciliationBatchCompletionV2::Incomplete
                                {
                                    let entry = batch.missing_suffix_entries().first().ok_or(
                                        Stage6ReconciliationV2Error::UnexpectedSuffixRecord,
                                    )?;
                                    if !entry.matches_record(v1) {
                                        return Err(
                                            Stage6ReconciliationV2Error::UnexpectedSuffixRecord,
                                        );
                                    }
                                    state.v1.apply(v1)?;
                                    batch.verified_suffix_prefix_length += 1;
                                    batch.last_mixed_record_id = v1.journal_record_id().clone();
                                    batch.last_mixed_lifecycle_sequence = v1.lifecycle_sequence();
                                    if batch.missing_suffix_entries().is_empty() {
                                        batch.completion =
                                            Stage6ReconciliationBatchCompletionV2::Complete;
                                    }
                                } else {
                                    state.v1.apply(v1)?;
                                }
                            } else {
                                state.v1.apply(v1)?;
                            }
                        }
                    }
                }
                Stage6JournalRecordVersioned::V2(v2) => {
                    let request_key = v2
                        .durable_request_identity()
                        .strategy_request_id()
                        .to_string();
                    let state = requests.get_mut(&request_key).ok_or(
                        Stage6ReconciliationV2Error::Replay(
                            Stage6ReplayError::SequenceStartInvalid,
                        ),
                    )?;
                    if state.v1.is_finalized() {
                        return Err(Stage6ReconciliationV2Error::V2AfterFinalization);
                    }
                    let transition_key =
                        v2.payload.stable_transition_key_sha256.as_str().to_string();
                    if let Some(existing) = seen_transition_keys.get(&transition_key) {
                        if existing != &canonical {
                            return Err(Stage6ReconciliationV2Error::PendingBatchConflict);
                        }
                    } else {
                        seen_transition_keys.insert(transition_key, canonical.clone());
                    }
                    if state.batch.is_some() {
                        return Err(Stage6ReconciliationV2Error::PendingBatchConflict);
                    }
                    state.v1.advance_causal_only(
                        v2.durable_request_identity(),
                        v2.lifecycle_sequence(),
                        v2.previous_record_id(),
                        v2.journal_record_id().clone(),
                    )?;
                    let completion = if v2.payload.deterministic_suffix_manifest.entries.is_empty()
                    {
                        Stage6ReconciliationBatchCompletionV2::Complete
                    } else {
                        Stage6ReconciliationBatchCompletionV2::Incomplete
                    };
                    state.batch = Some(Stage6PendingReconciliationBatchV2 {
                        transition_record: v2.clone(),
                        verified_suffix_prefix_length: 0,
                        completion,
                        last_mixed_record_id: v2.journal_record_id().clone(),
                        last_mixed_lifecycle_sequence: v2.lifecycle_sequence(),
                    });
                }
                Stage6JournalRecordVersioned::V3(v3) => {
                    if last_unique_record_id.as_ref() != Some(v3.previous_record_id()) {
                        return Err(Stage6ReconciliationV2Error::InvalidCausalEnvelope);
                    }
                    let transition_key = v3.transition_key();
                    if let Some(existing) = seen_p1d3_transition_keys.get(&transition_key) {
                        if existing != &canonical {
                            return Err(Stage6ReconciliationV2Error::PendingBatchConflict);
                        }
                    } else {
                        seen_p1d3_transition_keys.insert(transition_key, canonical.clone());
                    }
                    if let Some(effect) = crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeEvidenceV1::decode_canonical(
                        v3.outcome_evidence_bytes(),
                    )
                    .map_err(|_| Stage6ReconciliationV2Error::InvalidPayload)?
                    .stage6_request_replay_effect()
                    .map_err(|_| Stage6ReconciliationV2Error::InvalidPayload)?
                    {
                        let request_key = effect.request_id.to_string();
                        let state = requests.get_mut(&request_key).ok_or(
                            Stage6ReconciliationV2Error::Replay(
                                Stage6ReplayError::SequenceStartInvalid,
                            ),
                        )?;
                        if state.batch.is_some() {
                            return Err(Stage6ReconciliationV2Error::PendingBatchConflict);
                        }
                        state.v1.apply_stage8b_p1d3_outcome(
                            effect,
                            v3.lifecycle_sequence(),
                            v3.journal_record_id().clone(),
                        )?;
                    }
                    p1d3_outcome_records.push(v3.clone());
                }
                Stage6JournalRecordVersioned::V4(v4) => {
                    if last_unique_record_id.as_ref() != Some(v4.previous_record_id())
                        || last_unique_lifecycle_sequence
                            .and_then(|sequence| sequence.get().checked_add(1))
                            != Some(v4.lifecycle_sequence().get())
                    {
                        return Err(Stage6ReconciliationV2Error::InvalidCausalEnvelope);
                    }
                    let transition_key = v4.transition_key().to_string();
                    if let Some(existing) = seen_schedule_transition_keys.get(&transition_key) {
                        if existing != &canonical {
                            return Err(Stage6ReconciliationV2Error::PendingBatchConflict);
                        }
                    } else {
                        seen_schedule_transition_keys.insert(transition_key, canonical.clone());
                    }
                    if v4.transition_kind()
                        == crate::Stage8bP1eScheduleTransitionKindV1::MarketExecution
                    {
                        let binding = v4.request_or_order_binding();
                        let request_id = binding
                            .strategy_request_id
                            .as_ref()
                            .ok_or(Stage6ReconciliationV2Error::InvalidPayload)?;
                        let state = requests.get_mut(request_id).ok_or(
                            Stage6ReconciliationV2Error::Replay(
                                Stage6ReplayError::SequenceStartInvalid,
                            ),
                        )?;
                        if state.batch.is_some() {
                            return Err(Stage6ReconciliationV2Error::PendingBatchConflict);
                        }
                        let identity = last_unique_request_accepted_identity
                            .as_ref()
                            .filter(|identity| {
                                identity.action() == Stage6DurableActionKind::Place
                                    && identity.strategy_request_id().to_string() == *request_id
                            })
                            .ok_or(Stage6ReconciliationV2Error::InvalidCausalEnvelope)?;
                        state.v1.advance_causal_only(
                            identity,
                            v4.lifecycle_sequence(),
                            Some(v4.previous_record_id()),
                            v4.journal_record_id().clone(),
                        )?;
                    }
                    schedule_binding_records.push(v4.clone());
                }
            }
            last_unique_request_accepted_identity = match record {
                Stage6JournalRecordVersioned::V1(v1)
                    if v1.event_kind() == Stage6JournalEventKind::RequestAccepted =>
                {
                    Some(v1.durable_request_identity().clone())
                }
                _ => None,
            };
            seen.insert(key, canonical);
            last_unique_record_id = Some(record.journal_record_id().clone());
            last_unique_lifecycle_sequence = Some(record.lifecycle_sequence());
        }
        let mut recovered = Vec::new();
        let mut batches = Vec::new();
        for state in requests.into_values() {
            recovered.push(state.v1.into_recovered());
            if let Some(batch) = state.batch {
                batches.push(batch);
            }
        }
        Ok(Stage6MixedReplaySnapshotV2 {
            requests: recovered,
            reconciliation_batches: batches,
            p1d3_outcome_records,
            schedule_binding_records,
        })
    }
}

/// Read-only version-aware framed journal reader. There is intentionally no
/// corresponding V2 writer or append method in I1.
#[derive(Debug, Default)]
pub struct Stage6VersionedJournalReader;

impl Stage6VersionedJournalReader {
    pub fn read_framed_bytes(
        bytes: &[u8],
    ) -> Result<Vec<Stage6JournalRecordVersioned>, crate::Stage6JournalStorageError> {
        crate::stage6_journal_backend::scan_versioned_framed_bytes(bytes)
    }
}

/// Test-only source-exact I3 batch fixture. The feature is enabled only by
/// downstream durability tests; production builds expose no V2 constructor.
#[cfg(feature = "stage5g-artifact-fixtures")]
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn stage8a4_test_transition_fixture(
    identity: &Stage6DurableRequestIdentityV1,
    dispatch: &Stage6JournalRecordV1,
    durable_request_binding_sha256: Stage6Sha256Digest,
    expected_frontier: Stage6Sha256Digest,
    expected_seal_generation: u64,
    expected_seal_fingerprint: Stage6Sha256Digest,
    expected_request_state_fingerprint: Stage6Sha256Digest,
    suffix_count: usize,
) -> (Stage6JournalRecordV2, Vec<Stage6JournalRecordV1>) {
    let digest = |byte: char| {
        Stage6Sha256Digest::parse(byte.to_string().repeat(64)).expect("fixture digest")
    };
    let sequence = Stage6LifecycleSequence::new(
        dispatch
            .lifecycle_sequence()
            .get()
            .checked_add(1)
            .expect("fixture sequence"),
    )
    .expect("fixture sequence");
    let v2_id = Stage6JournalRecordId::derive(identity.strategy_request_id(), sequence);
    let mut suffix = Vec::new();
    if suffix_count > 0 {
        suffix.push(
            Stage6JournalRecordV1::broker_order_observed(
                identity.clone(),
                BrokerOrderId::new("ORDER-I3-1"),
                Stage6LifecycleSequence::new(sequence.get().checked_add(1).unwrap()).unwrap(),
                Some(v2_id.clone()),
                digest('c'),
            )
            .expect("fixture observation"),
        );
    }
    if suffix_count > 1 {
        suffix.push(
            Stage6JournalRecordV1::request_finalized(
                identity.clone(),
                crate::Stage6RequestFinalDispositionV1::Completed,
                Stage6LifecycleSequence::new(
                    suffix[0].lifecycle_sequence().get().checked_add(1).unwrap(),
                )
                .unwrap(),
                Some(suffix[0].journal_record_id().clone()),
                digest('c'),
            )
            .expect("fixture finalization"),
        );
    }
    assert!(suffix_count <= 2, "fixture suffix count is bounded");
    let manifest = suffix
        .iter()
        .enumerate()
        .map(|(index, record)| Stage6SuffixManifestEntryV2 {
            ordinal: index as u16,
            event_kind: record.event_kind(),
            journal_record_id: record.journal_record_id().clone(),
            lifecycle_sequence: record.lifecycle_sequence(),
            canonical_payload_sha256: record.canonical_payload_sha256().clone(),
            canonical_record_sha256: Stage6Sha256Digest::of(&record.encode_canonical()),
        })
        .collect();
    let received_ts = DateTime::from_timestamp(1_786_435_200, 0).expect("fixture timestamp");
    let (endpoint_kind, order_type, limit_price) = match identity.action() {
        Stage6DurableActionKind::Place => (
            Stage6ReconciliationEndpointKindV2::Place,
            OrderType::Market,
            None,
        ),
        Stage6DurableActionKind::Cancel => (
            Stage6ReconciliationEndpointKindV2::Cancel,
            OrderType::Limit,
            Some(Price::new(2210, 1)),
        ),
    };
    let payload = Stage6ReconciliationTransitionPayloadV2 {
        stable_transition_key_sha256: digest('3'),
        durable_request_binding_sha256,
        private_authoritative_outcome_binding_sha256: digest('5'),
        endpoint_kind,
        transition_kind: Stage6ReconciliationTransitionKindV2::Exact {
            lifecycle: Stage6ReconciliationLifecycleV2::Working,
        },
        exact_lookup_evidence: Stage6ExactLookupEvidenceV2::NotAttempted,
        broker_order_fact: Some(Stage6BrokerOrderFactV2 {
            account_id: identity.account_id().clone(),
            broker_order_id: Some(BrokerOrderId::new("ORDER-I3-1")),
            client_order_id: Some(
                identity
                    .target_order_client_order_id()
                    .unwrap_or_else(|| identity.durable_client_order_id())
                    .clone(),
            ),
            instrument: identity.instrument().clone(),
            side: OrderSide::Buy,
            order_type,
            time_in_force: Some(TimeInForce::Day),
            status: OrderStatus::Working,
            lifecycle: BrokerOrderLifecycle::Active,
            qty: Quantity::ONE,
            filled_qty: Quantity::ZERO,
            remaining_qty: Some(Quantity::ONE),
            limit_price,
            broker_asset_id: Some("ASSET-IMOEXF".into()),
            board: Some("RFUD".into()),
            expiration_date: None,
            source_ts: Some(received_ts),
            received_ts,
        }),
        material_trade_facts: Vec::new(),
        fill_effect: Stage6ReconciliationFillEffectV2::Zero,
        account_safety_summary: Stage6AccountSafetySummaryV2 {
            account_active_orders_count: 1,
            account_unknown_orders_count: 0,
            account_orphan_orders_count: 0,
            account_open_positions_count: 0,
            target_active_orders_count: 1,
            target_unknown_orders_count: 0,
            target_terminal_orders_count: 0,
            target_inconsistent_orders_count: 0,
            target_open_positions_count: 0,
            other_symbol_active_orders_count: 0,
            account_safety_binding_sha256: digest('6'),
        },
        pre_append_precondition: Stage6PreAppendPreconditionV2 {
            expected_stage6_checkpoint_or_frontier_fingerprint: expected_frontier,
            expected_recovery_seal_generation: expected_seal_generation,
            expected_recovery_seal_fingerprint: expected_seal_fingerprint,
            expected_request_state_fingerprint,
        },
        deterministic_suffix_manifest: Stage6SuffixManifestV2 { entries: manifest },
    };
    let canonical_payload_sha256 =
        Stage6Sha256Digest::of(&serde_json::to_vec(&payload).expect("fixture payload"));
    let value = Stage6JournalRecordV2 {
        schema_version: STAGE6_DURABLE_RECORD_SCHEMA_VERSION_V2,
        journal_record_id: Stage6JournalRecordId::derive(identity.strategy_request_id(), sequence),
        lifecycle_sequence: sequence,
        previous_record_id: Some(dispatch.journal_record_id().clone()),
        causal_parent_id: Some(dispatch.journal_record_id().clone()),
        durable_request_identity: identity.clone(),
        event_kind: Stage6JournalEventKindV2::ReconciliationTransitionApplied,
        payload,
        canonical_payload_sha256,
        source_evidence_sha256: digest('c'),
    };
    value.validate().expect("fixture V2 validates");
    (value, suffix)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        Stage6DurableCommandSnapshotV1, Stage6JournalBackend, Stage6JournalRecordV1,
        Stage6RequestFinalDispositionV1,
    };
    use broker_core::{
        CancelOrder, Exchange, HybridRuntimeAttribution, Market, PlaceOrder, StrategyRequestId,
    };
    use chrono::TimeZone;
    use rust_decimal::Decimal;
    use serde_json::Value;
    use uuid::Uuid;

    fn digest(byte: char) -> Stage6Sha256Digest {
        Stage6Sha256Digest::parse(byte.to_string().repeat(64)).unwrap()
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 15, 9, 0, 0).unwrap()
    }

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".into(),
            venue_symbol: Some("IMOEXF@RTSX".into()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn place_fixture() -> (
        Stage6DurableRequestIdentityV1,
        Stage6JournalRecordV1,
        Stage6JournalRecordV1,
    ) {
        let request_id =
            StrategyRequestId::from(Uuid::from_u128(0x11111111111111111111111111111111));
        let attribution = HybridRuntimeAttribution::parse_source_comment(
            "HYB|sid=hybrid_imoexf|c=cycle0001|o=BO|r=ENTRY",
        )
        .unwrap();
        let command = PlaceOrder {
            request_id,
            created_ts: now(),
            ttl_ms: Some(5_000),
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            client_order_id: ClientOrderId::from_strategy_request(request_id),
            instrument: instrument(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            qty: Decimal::ONE,
            limit_price: Some(Decimal::new(2210, 1)),
            time_in_force: TimeInForce::Day,
            comment: Some(attribution.internal_comment().to_string()),
        };
        let identity = Stage6DurableRequestIdentityV1::from_place(&command, attribution).unwrap();
        let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, &command).unwrap();
        let accepted = Stage6JournalRecordV1::request_accepted(
            identity.clone(),
            snapshot,
            Stage6LifecycleSequence::new(1).unwrap(),
            None,
            None,
            digest('1'),
        )
        .unwrap();
        let attempt = Stage6JournalRecordV1::dispatch_attempt_recorded(
            identity.clone(),
            1,
            accepted.canonical_payload_sha256().clone(),
            Stage6LifecycleSequence::new(2).unwrap(),
            Some(accepted.journal_record_id().clone()),
            digest('2'),
        )
        .unwrap();
        (identity, accepted, attempt)
    }

    fn cancel_fixture() -> (
        Stage6DurableRequestIdentityV1,
        Stage6JournalRecordV1,
        Stage6JournalRecordV1,
    ) {
        let request_id =
            StrategyRequestId::from(Uuid::from_u128(0x22222222222222222222222222222222));
        let attribution = HybridRuntimeAttribution::parse_source_comment(
            "HYB|sid=hybrid_imoexf|c=cycle0002|o=BO|r=CANCEL",
        )
        .unwrap();
        let command = CancelOrder {
            request_id,
            created_ts: now(),
            ttl_ms: Some(5_000),
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            order_id: BrokerOrderId::new("ORDER-1"),
            client_order_id: Some(ClientOrderId::from_strategy_request(
                StrategyRequestId::from(Uuid::from_u128(0x11111111111111111111111111111111)),
            )),
        };
        let identity =
            Stage6DurableRequestIdentityV1::from_cancel(&command, instrument(), attribution)
                .unwrap();
        let snapshot = Stage6DurableCommandSnapshotV1::from_cancel(&identity, &command).unwrap();
        let accepted = Stage6JournalRecordV1::request_accepted(
            identity.clone(),
            snapshot,
            Stage6LifecycleSequence::new(1).unwrap(),
            None,
            None,
            digest('1'),
        )
        .unwrap();
        let attempt = Stage6JournalRecordV1::dispatch_attempt_recorded(
            identity.clone(),
            1,
            accepted.canonical_payload_sha256().clone(),
            Stage6LifecycleSequence::new(2).unwrap(),
            Some(accepted.journal_record_id().clone()),
            digest('2'),
        )
        .unwrap();
        (identity, accepted, attempt)
    }

    fn order_fact(
        identity: &Stage6DurableRequestIdentityV1,
        broker_id: Option<&str>,
    ) -> Stage6BrokerOrderFactV2 {
        Stage6BrokerOrderFactV2 {
            account_id: identity.account_id().clone(),
            broker_order_id: broker_id.map(BrokerOrderId::new),
            client_order_id: Some(
                identity
                    .target_order_client_order_id()
                    .unwrap_or_else(|| identity.durable_client_order_id())
                    .clone(),
            ),
            instrument: identity.instrument().clone(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            time_in_force: Some(TimeInForce::Day),
            status: OrderStatus::Working,
            lifecycle: BrokerOrderLifecycle::Active,
            qty: Decimal::ONE,
            filled_qty: Decimal::ZERO,
            remaining_qty: Some(Decimal::ONE),
            limit_price: Some(Decimal::new(2210, 1)),
            broker_asset_id: Some("ASSET-IMOEXF".into()),
            board: Some("RFUD".into()),
            expiration_date: None,
            source_ts: Some(now()),
            received_ts: now(),
        }
    }

    fn trade_fact(
        identity: &Stage6DurableRequestIdentityV1,
        broker_order_id: Option<&str>,
    ) -> Stage6MaterialTradeFactV2 {
        Stage6MaterialTradeFactV2 {
            account_id: identity.account_id().clone(),
            broker_trade_id: BrokerTradeId::new("TRADE-1"),
            broker_order_id: broker_order_id.map(BrokerOrderId::new),
            client_order_id: Some(identity.durable_client_order_id().clone()),
            instrument: identity.instrument().clone(),
            side: OrderSide::Buy,
            qty: Decimal::new(5, 1),
            price: Decimal::new(2210, 1),
            gross_amount: Some(Decimal::new(1105, 1)),
            commission: Some(Decimal::new(1, 2)),
            broker_asset_id: Some("ASSET-IMOEXF".into()),
            board: Some("RFUD".into()),
            expiration_date: None,
            source_ts: now(),
            received_ts: now(),
        }
    }

    fn manifest_entry(ordinal: u16, record: &Stage6JournalRecordV1) -> Stage6SuffixManifestEntryV2 {
        Stage6SuffixManifestEntryV2 {
            ordinal,
            event_kind: record.event_kind(),
            journal_record_id: record.journal_record_id().clone(),
            lifecycle_sequence: record.lifecycle_sequence(),
            canonical_payload_sha256: record.canonical_payload_sha256().clone(),
            canonical_record_sha256: Stage6Sha256Digest::of(&record.encode_canonical()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn payload(
        identity: &Stage6DurableRequestIdentityV1,
        endpoint_kind: Stage6ReconciliationEndpointKindV2,
        transition_kind: Stage6ReconciliationTransitionKindV2,
        lookup: Stage6ExactLookupEvidenceV2,
        broker_id: Option<&str>,
        trades: Vec<Stage6MaterialTradeFactV2>,
        fill_effect: Stage6ReconciliationFillEffectV2,
        suffix: Vec<Stage6SuffixManifestEntryV2>,
    ) -> Stage6ReconciliationTransitionPayloadV2 {
        Stage6ReconciliationTransitionPayloadV2 {
            stable_transition_key_sha256: digest('3'),
            durable_request_binding_sha256: digest('4'),
            private_authoritative_outcome_binding_sha256: digest('5'),
            endpoint_kind,
            transition_kind,
            exact_lookup_evidence: lookup,
            broker_order_fact: Some(order_fact(identity, broker_id)),
            material_trade_facts: trades,
            fill_effect,
            account_safety_summary: Stage6AccountSafetySummaryV2 {
                account_active_orders_count: 1,
                account_unknown_orders_count: 0,
                account_orphan_orders_count: 0,
                account_open_positions_count: 0,
                target_active_orders_count: 1,
                target_unknown_orders_count: 0,
                target_terminal_orders_count: 0,
                target_inconsistent_orders_count: 0,
                target_open_positions_count: 0,
                other_symbol_active_orders_count: 0,
                account_safety_binding_sha256: digest('6'),
            },
            pre_append_precondition: Stage6PreAppendPreconditionV2 {
                expected_stage6_checkpoint_or_frontier_fingerprint: digest('7'),
                expected_recovery_seal_generation: 9,
                expected_recovery_seal_fingerprint: digest('8'),
                expected_request_state_fingerprint: digest('9'),
            },
            deterministic_suffix_manifest: Stage6SuffixManifestV2 { entries: suffix },
        }
    }

    fn broker_lifecycle_for_status(status: &OrderStatus) -> BrokerOrderLifecycle {
        match status {
            OrderStatus::New | OrderStatus::Working | OrderStatus::PartiallyFilled => {
                BrokerOrderLifecycle::Active
            }
            OrderStatus::Filled
            | OrderStatus::Canceled
            | OrderStatus::Rejected
            | OrderStatus::Expired => BrokerOrderLifecycle::Terminal,
            OrderStatus::Unknown(_) => BrokerOrderLifecycle::Unknown,
        }
    }

    fn exact_payload_for_state(
        identity: &Stage6DurableRequestIdentityV1,
        lifecycle: Stage6ReconciliationLifecycleV2,
        status: OrderStatus,
        fill_effect: Stage6ReconciliationFillEffectV2,
    ) -> Stage6ReconciliationTransitionPayloadV2 {
        let filled_qty = match &fill_effect {
            Stage6ReconciliationFillEffectV2::Zero => Decimal::ZERO,
            Stage6ReconciliationFillEffectV2::Partial { filled_qty }
            | Stage6ReconciliationFillEffectV2::Full { filled_qty } => *filled_qty,
        };
        let mut trades = Vec::new();
        if filled_qty > Decimal::ZERO {
            let mut trade = trade_fact(identity, Some("ORDER-1"));
            trade.qty = filled_qty;
            trades.push(trade);
        }
        let mut candidate = payload(
            identity,
            Stage6ReconciliationEndpointKindV2::Place,
            Stage6ReconciliationTransitionKindV2::Exact { lifecycle },
            Stage6ExactLookupEvidenceV2::NotAttempted,
            Some("ORDER-1"),
            trades,
            fill_effect,
            Vec::new(),
        );
        let order = candidate.broker_order_fact.as_mut().unwrap();
        order.lifecycle = broker_lifecycle_for_status(&status);
        order.status = status;
        order.filled_qty = filled_qty;
        order.remaining_qty = Some(order.qty - filled_qty);
        candidate
    }

    fn unchecked_record_bytes(
        identity: Stage6DurableRequestIdentityV1,
        attempt: &Stage6JournalRecordV1,
        payload: Stage6ReconciliationTransitionPayloadV2,
    ) -> Vec<u8> {
        let sequence = Stage6LifecycleSequence::new(3).unwrap();
        let canonical_payload_sha256 =
            Stage6Sha256Digest::of(&serde_json::to_vec(&payload).unwrap());
        Stage6JournalRecordV2 {
            schema_version: STAGE6_DURABLE_RECORD_SCHEMA_VERSION_V2,
            journal_record_id: Stage6JournalRecordId::derive(
                identity.strategy_request_id(),
                sequence,
            ),
            lifecycle_sequence: sequence,
            previous_record_id: Some(attempt.journal_record_id().clone()),
            causal_parent_id: Some(attempt.journal_record_id().clone()),
            durable_request_identity: identity,
            event_kind: Stage6JournalEventKindV2::ReconciliationTransitionApplied,
            payload,
            canonical_payload_sha256,
            source_evidence_sha256: digest('c'),
        }
        .encode_canonical()
    }

    fn v2_with_suffix(
        identity: &Stage6DurableRequestIdentityV1,
        attempt: &Stage6JournalRecordV1,
        suffix_kind: usize,
    ) -> (Stage6JournalRecordV2, Vec<Stage6JournalRecordV1>) {
        let sequence = Stage6LifecycleSequence::new(3).unwrap();
        let v2_id = Stage6JournalRecordId::derive(identity.strategy_request_id(), sequence);
        let mut suffix = Vec::new();
        if suffix_kind > 0 {
            suffix.push(
                Stage6JournalRecordV1::broker_order_observed(
                    identity.clone(),
                    BrokerOrderId::new("ORDER-1"),
                    Stage6LifecycleSequence::new(4).unwrap(),
                    Some(v2_id.clone()),
                    digest('a'),
                )
                .unwrap(),
            );
        }
        if suffix_kind > 1 {
            suffix.push(
                Stage6JournalRecordV1::request_finalized(
                    identity.clone(),
                    Stage6RequestFinalDispositionV1::Completed,
                    Stage6LifecycleSequence::new(5).unwrap(),
                    Some(suffix[0].journal_record_id().clone()),
                    digest('b'),
                )
                .unwrap(),
            );
        }
        let manifest = suffix
            .iter()
            .enumerate()
            .map(|(index, record)| manifest_entry(index as u16, record))
            .collect();
        let record = Stage6JournalRecordV2::build_for_test(
            identity.clone(),
            sequence,
            attempt.journal_record_id().clone(),
            payload(
                identity,
                Stage6ReconciliationEndpointKindV2::Place,
                Stage6ReconciliationTransitionKindV2::Exact {
                    lifecycle: Stage6ReconciliationLifecycleV2::Working,
                },
                Stage6ExactLookupEvidenceV2::NotAttempted,
                Some("ORDER-1"),
                Vec::new(),
                Stage6ReconciliationFillEffectV2::Zero,
                manifest,
            ),
            digest('c'),
        );
        (record, suffix)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn i3_batch_fixture(
        identity: &Stage6DurableRequestIdentityV1,
        attempt: &Stage6JournalRecordV1,
        durable_request_binding_sha256: Stage6Sha256Digest,
        expected_frontier: Stage6Sha256Digest,
        expected_seal_generation: u64,
        expected_seal_fingerprint: Stage6Sha256Digest,
        expected_request_state_fingerprint: Stage6Sha256Digest,
        suffix_kind: usize,
    ) -> (Stage6JournalRecordV2, Vec<Stage6JournalRecordV1>) {
        let sequence = Stage6LifecycleSequence::new(
            attempt.lifecycle_sequence().get().checked_add(1).unwrap(),
        )
        .unwrap();
        let v2_id = Stage6JournalRecordId::derive(identity.strategy_request_id(), sequence);
        let mut suffix = Vec::new();
        if suffix_kind > 0 {
            suffix.push(
                Stage6JournalRecordV1::broker_order_observed(
                    identity.clone(),
                    BrokerOrderId::new("ORDER-I3-1"),
                    Stage6LifecycleSequence::new(sequence.get().checked_add(1).unwrap()).unwrap(),
                    Some(v2_id.clone()),
                    digest('a'),
                )
                .unwrap(),
            );
        }
        if suffix_kind > 1 {
            suffix.push(
                Stage6JournalRecordV1::request_finalized(
                    identity.clone(),
                    Stage6RequestFinalDispositionV1::Completed,
                    Stage6LifecycleSequence::new(
                        suffix[0].lifecycle_sequence().get().checked_add(1).unwrap(),
                    )
                    .unwrap(),
                    Some(suffix[0].journal_record_id().clone()),
                    digest('b'),
                )
                .unwrap(),
            );
        }
        let manifest = suffix
            .iter()
            .enumerate()
            .map(|(index, record)| manifest_entry(index as u16, record))
            .collect();
        let mut payload = payload(
            identity,
            match identity.action() {
                Stage6DurableActionKind::Place => Stage6ReconciliationEndpointKindV2::Place,
                Stage6DurableActionKind::Cancel => Stage6ReconciliationEndpointKindV2::Cancel,
            },
            Stage6ReconciliationTransitionKindV2::Exact {
                lifecycle: Stage6ReconciliationLifecycleV2::Working,
            },
            Stage6ExactLookupEvidenceV2::NotAttempted,
            Some("ORDER-I3-1"),
            Vec::new(),
            Stage6ReconciliationFillEffectV2::Zero,
            manifest,
        );
        payload.pre_append_precondition = Stage6PreAppendPreconditionV2 {
            expected_stage6_checkpoint_or_frontier_fingerprint: expected_frontier,
            expected_recovery_seal_generation: expected_seal_generation,
            expected_recovery_seal_fingerprint: expected_seal_fingerprint,
            expected_request_state_fingerprint,
        };
        payload.durable_request_binding_sha256 = durable_request_binding_sha256;
        let record = Stage6JournalRecordV2::build_for_test(
            identity.clone(),
            sequence,
            attempt.journal_record_id().clone(),
            payload,
            digest('c'),
        );
        (record, suffix)
    }

    fn golden_record(
        identity: &Stage6DurableRequestIdentityV1,
        attempt: &Stage6JournalRecordV1,
        payload: Stage6ReconciliationTransitionPayloadV2,
    ) -> Vec<u8> {
        Stage6JournalRecordV2::build_for_test(
            identity.clone(),
            Stage6LifecycleSequence::new(3).unwrap(),
            attempt.journal_record_id().clone(),
            payload,
            digest('c'),
        )
        .encode_canonical()
    }

    fn lookup_variants(
        identity: &Stage6DurableRequestIdentityV1,
    ) -> Vec<(&'static str, Stage6ExactLookupEvidenceV2)> {
        let observation = Stage6ExactOrderObservationV2 {
            order: order_fact(identity, Some("ORDER-1")),
            observation_binding_sha256: digest('d'),
        };
        vec![
            (
                "ExactLookupNotAttempted",
                Stage6ExactLookupEvidenceV2::NotAttempted,
            ),
            (
                "ExactLookupSucceededWithObservation",
                Stage6ExactLookupEvidenceV2::Succeeded {
                    account_id: identity.account_id().clone(),
                    queried_broker_order_id: BrokerOrderId::new("ORDER-1"),
                    durable_request_binding_sha256: digest('4'),
                    request_started_at: now(),
                    response_received_at: now(),
                    exact_order_observation_v2: observation,
                },
            ),
            (
                "ExactLookupDocumentedNotFound",
                Stage6ExactLookupEvidenceV2::DocumentedNotFound {
                    account_id: identity.account_id().clone(),
                    queried_broker_order_id: BrokerOrderId::new("ORDER-1"),
                    durable_request_binding_sha256: digest('4'),
                    request_started_at: now(),
                    response_received_at: now(),
                    documented_status_category: "documented_not_found".into(),
                },
            ),
            (
                "ExactLookupUnavailable",
                Stage6ExactLookupEvidenceV2::Unavailable {
                    account_id: identity.account_id().clone(),
                    queried_broker_order_id: BrokerOrderId::new("ORDER-1"),
                    durable_request_binding_sha256: digest('4'),
                    request_started_at: now(),
                    response_received_at: now(),
                    failure_category: "timeout".into(),
                },
            ),
            (
                "ExactLookupDecodeFailure",
                Stage6ExactLookupEvidenceV2::DecodeFailure {
                    account_id: identity.account_id().clone(),
                    queried_broker_order_id: BrokerOrderId::new("ORDER-1"),
                    durable_request_binding_sha256: digest('4'),
                    request_started_at: now(),
                    response_received_at: now(),
                    response_status_category: "success_2xx".into(),
                    response_binding_sha256: digest('e'),
                },
            ),
            (
                "ExactLookupStale",
                Stage6ExactLookupEvidenceV2::Stale {
                    account_id: identity.account_id().clone(),
                    queried_broker_order_id: BrokerOrderId::new("ORDER-1"),
                    durable_request_binding_sha256: digest('4'),
                    request_started_at: now(),
                    response_received_at: now(),
                    stale_observation_binding_sha256: digest('f'),
                },
            ),
        ]
    }

    fn canonical_golden_cases() -> Vec<(&'static str, Vec<u8>)> {
        let (place_identity, accepted, attempt) = place_fixture();
        let mut cases = Vec::new();

        let working = payload(
            &place_identity,
            Stage6ReconciliationEndpointKindV2::Place,
            Stage6ReconciliationTransitionKindV2::Exact {
                lifecycle: Stage6ReconciliationLifecycleV2::Working,
            },
            Stage6ExactLookupEvidenceV2::NotAttempted,
            Some("ORDER-1"),
            Vec::new(),
            Stage6ReconciliationFillEffectV2::Zero,
            Vec::new(),
        );
        cases.push((
            "PlaceExactWorkingBrokerOrderIdPresent",
            golden_record(&place_identity, &attempt, working.clone()),
        ));
        let mut working_without_id = working.clone();
        working_without_id
            .broker_order_fact
            .as_mut()
            .unwrap()
            .broker_order_id = None;
        cases.push((
            "PlaceExactWorkingBrokerOrderIdAbsent",
            golden_record(&place_identity, &attempt, working_without_id),
        ));

        let mut rejected = working.clone();
        rejected.transition_kind = Stage6ReconciliationTransitionKindV2::Exact {
            lifecycle: Stage6ReconciliationLifecycleV2::TerminalRejected,
        };
        let rejected_order = rejected.broker_order_fact.as_mut().unwrap();
        rejected_order.status = OrderStatus::Rejected;
        rejected_order.lifecycle = BrokerOrderLifecycle::Terminal;
        cases.push((
            "PlaceExactTerminalRejected",
            golden_record(&place_identity, &attempt, rejected),
        ));

        let mut partial = working.clone();
        let partial_order = partial.broker_order_fact.as_mut().unwrap();
        partial_order.status = OrderStatus::PartiallyFilled;
        partial_order.filled_qty = Decimal::new(5, 1);
        partial_order.remaining_qty = Some(Decimal::new(5, 1));
        partial.material_trade_facts = vec![trade_fact(&place_identity, Some("ORDER-1"))];
        partial.fill_effect = Stage6ReconciliationFillEffectV2::Partial {
            filled_qty: Decimal::new(5, 1),
        };
        cases.push((
            "PlacePartialFillTradeBrokerOrderIdPresent",
            golden_record(&place_identity, &attempt, partial.clone()),
        ));
        partial.broker_order_fact.as_mut().unwrap().broker_order_id = None;
        partial.material_trade_facts[0].broker_order_id = None;
        cases.push((
            "PlacePartialFillClientLinkedTradeBrokerOrderIdAbsent",
            golden_record(&place_identity, &attempt, partial),
        ));

        let (cancel_identity, _, cancel_attempt) = cancel_fixture();
        let mut cancel_working = payload(
            &cancel_identity,
            Stage6ReconciliationEndpointKindV2::Cancel,
            Stage6ReconciliationTransitionKindV2::Exact {
                lifecycle: Stage6ReconciliationLifecycleV2::Working,
            },
            Stage6ExactLookupEvidenceV2::NotAttempted,
            Some("ORDER-1"),
            Vec::new(),
            Stage6ReconciliationFillEffectV2::Zero,
            Vec::new(),
        );
        cancel_working
            .broker_order_fact
            .as_mut()
            .unwrap()
            .client_order_id = cancel_identity.target_order_client_order_id().cloned();
        cases.push((
            "CancelExactWorking",
            golden_record(&cancel_identity, &cancel_attempt, cancel_working.clone()),
        ));
        let mut cancel_terminal = cancel_working;
        cancel_terminal.transition_kind = Stage6ReconciliationTransitionKindV2::Exact {
            lifecycle: Stage6ReconciliationLifecycleV2::TerminalCancelled,
        };
        let cancel_order = cancel_terminal.broker_order_fact.as_mut().unwrap();
        cancel_order.status = OrderStatus::Canceled;
        cancel_order.lifecycle = BrokerOrderLifecycle::Terminal;
        cases.push((
            "CancelTerminalCancelled",
            golden_record(&cancel_identity, &cancel_attempt, cancel_terminal),
        ));

        for (name, transition) in [
            (
                "ConflictHold",
                Stage6ReconciliationTransitionKindV2::ReconciliationConflictHold,
            ),
            (
                "StillUnknownHold",
                Stage6ReconciliationTransitionKindV2::ReconciliationStillUnknownHold,
            ),
        ] {
            let mut held = working.clone();
            held.transition_kind = transition;
            cases.push((name, golden_record(&place_identity, &attempt, held)));
        }

        for (name, lookup) in lookup_variants(&place_identity) {
            let mut looked_up = working.clone();
            looked_up.transition_kind =
                Stage6ReconciliationTransitionKindV2::ReconciliationConflictHold;
            looked_up.exact_lookup_evidence = lookup;
            cases.push((name, golden_record(&place_identity, &attempt, looked_up)));
        }

        let (empty_v2, _) = v2_with_suffix(&place_identity, &attempt, 0);
        cases.push((
            "MixedV1V2",
            crate::stage6_journal_backend::frame_versioned_records_for_test(&[
                Stage6JournalRecordVersioned::V1(accepted.clone()),
                Stage6JournalRecordVersioned::V1(attempt.clone()),
                Stage6JournalRecordVersioned::V2(empty_v2),
            ]),
        ));
        let (suffix_v2, suffix) = v2_with_suffix(&place_identity, &attempt, 2);
        cases.push((
            "MixedV1V2PartialV1Suffix",
            crate::stage6_journal_backend::frame_versioned_records_for_test(&[
                Stage6JournalRecordVersioned::V1(accepted.clone()),
                Stage6JournalRecordVersioned::V1(attempt.clone()),
                Stage6JournalRecordVersioned::V2(suffix_v2.clone()),
                Stage6JournalRecordVersioned::V1(suffix[0].clone()),
            ]),
        ));
        cases.push((
            "MixedV1V2CompleteV1Suffix",
            crate::stage6_journal_backend::frame_versioned_records_for_test(&[
                Stage6JournalRecordVersioned::V1(accepted),
                Stage6JournalRecordVersioned::V1(attempt),
                Stage6JournalRecordVersioned::V2(suffix_v2),
                Stage6JournalRecordVersioned::V1(suffix[0].clone()),
                Stage6JournalRecordVersioned::V1(suffix[1].clone()),
            ]),
        ));

        let mut unknown: Value = serde_json::from_slice(&cases[0].1).unwrap();
        unknown["schema_version"] = Value::from(4);
        cases.push((
            "UnknownRecordSchemaVersionFailClosed",
            serde_json::to_vec(&unknown).unwrap(),
        ));
        cases.push((
            "V1GoldenBytesUnchanged",
            include_bytes!("../../../fixtures/stage6a/place-request-accepted-v1.json")
                .strip_suffix(b"\n")
                .unwrap()
                .to_vec(),
        ));
        cases
    }

    #[test]
    fn canonical_golden_matrix_is_stable() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../fixtures/stage8a4-i1/canonical-golden-sha256.json"
        ))
        .unwrap();
        let expected = fixture["canonical_cases"].as_object().unwrap();
        let actual = canonical_golden_cases();
        assert_eq!(actual.len(), 20);
        assert_eq!(expected.len(), actual.len());
        for (name, bytes) in actual {
            assert_eq!(
                expected[name].as_str().unwrap(),
                Stage6Sha256Digest::of(&bytes).as_str(),
                "canonical golden changed: {name}"
            );
        }
    }

    #[test]
    fn canonical_decoder_rejects_invalid_exact_status_fill_cross_product() {
        let (identity, _, attempt) = place_fixture();
        let partial = Stage6ReconciliationFillEffectV2::Partial {
            filled_qty: Decimal::new(5, 1),
        };
        let full = Stage6ReconciliationFillEffectV2::Full {
            filled_qty: Decimal::ONE,
        };
        let invalid = vec![
            (
                "filled-zero",
                Stage6ReconciliationLifecycleV2::TerminalFilled,
                OrderStatus::Filled,
                Stage6ReconciliationFillEffectV2::Zero,
            ),
            (
                "filled-partial",
                Stage6ReconciliationLifecycleV2::TerminalFilled,
                OrderStatus::Filled,
                partial.clone(),
            ),
            (
                "rejected-partial",
                Stage6ReconciliationLifecycleV2::TerminalRejected,
                OrderStatus::Rejected,
                partial.clone(),
            ),
            (
                "rejected-full",
                Stage6ReconciliationLifecycleV2::TerminalRejected,
                OrderStatus::Rejected,
                full.clone(),
            ),
            (
                "cancelled-full",
                Stage6ReconciliationLifecycleV2::TerminalCancelled,
                OrderStatus::Canceled,
                full.clone(),
            ),
            (
                "expired-full",
                Stage6ReconciliationLifecycleV2::TerminalExpired,
                OrderStatus::Expired,
                full.clone(),
            ),
            (
                "new-partial",
                Stage6ReconciliationLifecycleV2::Working,
                OrderStatus::New,
                partial.clone(),
            ),
            (
                "working-partial",
                Stage6ReconciliationLifecycleV2::Working,
                OrderStatus::Working,
                partial.clone(),
            ),
            (
                "partially-filled-zero",
                Stage6ReconciliationLifecycleV2::Working,
                OrderStatus::PartiallyFilled,
                Stage6ReconciliationFillEffectV2::Zero,
            ),
            (
                "partially-filled-full",
                Stage6ReconciliationLifecycleV2::Working,
                OrderStatus::PartiallyFilled,
                full,
            ),
            (
                "declared-lifecycle-drift",
                Stage6ReconciliationLifecycleV2::TerminalFilled,
                OrderStatus::Working,
                Stage6ReconciliationFillEffectV2::Zero,
            ),
        ];
        for (name, lifecycle, status, fill_effect) in invalid {
            let payload = exact_payload_for_state(&identity, lifecycle, status, fill_effect);
            assert_eq!(
                Stage6JournalRecordV2::decode_canonical(&unchecked_record_bytes(
                    identity.clone(),
                    &attempt,
                    payload,
                ))
                .unwrap_err(),
                Stage6ReconciliationV2Error::InvalidPayload,
                "invalid exact state survived canonical decode: {name}"
            );
        }
    }

    #[test]
    fn accepted_exact_status_fill_matrix_remains_canonical() {
        let (identity, _, attempt) = place_fixture();
        let partial = Stage6ReconciliationFillEffectV2::Partial {
            filled_qty: Decimal::new(5, 1),
        };
        let full = Stage6ReconciliationFillEffectV2::Full {
            filled_qty: Decimal::ONE,
        };
        let valid = vec![
            (
                Stage6ReconciliationLifecycleV2::Working,
                OrderStatus::New,
                Stage6ReconciliationFillEffectV2::Zero,
            ),
            (
                Stage6ReconciliationLifecycleV2::Working,
                OrderStatus::Working,
                Stage6ReconciliationFillEffectV2::Zero,
            ),
            (
                Stage6ReconciliationLifecycleV2::Working,
                OrderStatus::PartiallyFilled,
                partial.clone(),
            ),
            (
                Stage6ReconciliationLifecycleV2::TerminalFilled,
                OrderStatus::Filled,
                full,
            ),
            (
                Stage6ReconciliationLifecycleV2::TerminalRejected,
                OrderStatus::Rejected,
                Stage6ReconciliationFillEffectV2::Zero,
            ),
            (
                Stage6ReconciliationLifecycleV2::TerminalCancelled,
                OrderStatus::Canceled,
                Stage6ReconciliationFillEffectV2::Zero,
            ),
            (
                Stage6ReconciliationLifecycleV2::TerminalCancelled,
                OrderStatus::Canceled,
                partial.clone(),
            ),
            (
                Stage6ReconciliationLifecycleV2::TerminalExpired,
                OrderStatus::Expired,
                Stage6ReconciliationFillEffectV2::Zero,
            ),
            (
                Stage6ReconciliationLifecycleV2::TerminalExpired,
                OrderStatus::Expired,
                partial,
            ),
        ];
        for (lifecycle, status, fill_effect) in valid {
            let payload = exact_payload_for_state(&identity, lifecycle, status, fill_effect);
            let record = Stage6JournalRecordV2::build_for_test(
                identity.clone(),
                Stage6LifecycleSequence::new(3).unwrap(),
                attempt.journal_record_id().clone(),
                payload,
                digest('c'),
            );
            assert_eq!(
                Stage6JournalRecordV2::decode_canonical(&record.encode_canonical()).unwrap(),
                record
            );
        }
    }

    #[test]
    fn v1_golden_bytes_and_record_identity_remain_unchanged() {
        let bytes = include_bytes!("../../../fixtures/stage6a/place-request-accepted-v1.json")
            .strip_suffix(b"\n")
            .unwrap();
        let decoded = Stage6JournalRecordVersioned::decode_canonical(bytes).unwrap();
        assert!(matches!(decoded, Stage6JournalRecordVersioned::V1(_)));
        assert_eq!(decoded.encode_canonical(), bytes);
    }

    #[test]
    fn version_dispatch_is_exact_and_never_falls_back() {
        let (identity, _, attempt) = place_fixture();
        let (v2, _) = v2_with_suffix(&identity, &attempt, 0);
        assert!(matches!(
            Stage6JournalRecordVersioned::decode_canonical(&v2.encode_canonical()).unwrap(),
            Stage6JournalRecordVersioned::V2(_)
        ));

        let mut unknown: Value = serde_json::from_slice(&v2.encode_canonical()).unwrap();
        unknown["schema_version"] = Value::from(5);
        assert_eq!(
            Stage6JournalRecordVersioned::decode_canonical(&serde_json::to_vec(&unknown).unwrap())
                .unwrap_err(),
            Stage6ReconciliationV2Error::UnsupportedSchema(5)
        );

        let mut malformed_schema: Value = serde_json::from_slice(&v2.encode_canonical()).unwrap();
        malformed_schema["schema_version"] = Value::from("2");
        assert!(Stage6JournalRecordVersioned::decode_canonical(
            &serde_json::to_vec(&malformed_schema).unwrap()
        )
        .is_err());

        let duplicate = String::from_utf8(v2.encode_canonical()).unwrap().replacen(
            "{\"schema_version\":2,",
            "{\"schema_version\":2,\"schema_version\":1,",
            1,
        );
        assert_eq!(
            Stage6JournalRecordVersioned::decode_canonical(duplicate.as_bytes()).unwrap_err(),
            Stage6ReconciliationV2Error::AmbiguousSchema
        );

        let mut malformed: Value = serde_json::from_slice(&v2.encode_canonical()).unwrap();
        malformed
            .as_object_mut()
            .unwrap()
            .remove("canonical_payload_sha256");
        assert_eq!(
            Stage6JournalRecordVersioned::decode_canonical(
                &serde_json::to_vec(&malformed).unwrap()
            )
            .unwrap_err(),
            Stage6ReconciliationV2Error::DecodeFailed
        );
    }

    #[test]
    fn v4_schedule_binding_roundtrips_and_fails_closed_on_wire_drift() {
        let candidate = crate::stage8b_p1e_test_working_limit_binding_candidate("d".repeat(64));
        let record = Stage6JournalRecordV4::from_stage8b_p1e_candidate(
            &candidate,
            Stage6LifecycleSequence::new(3).unwrap(),
            Stage6JournalRecordId::parse_exact("e".repeat(64)).unwrap(),
            7,
            DateTime::parse_from_rfc3339("2026-09-14T12:30:00.000000Z")
                .unwrap()
                .with_timezone(&Utc),
        )
        .unwrap();
        let canonical = record.encode_canonical();
        assert_eq!(
            Stage6JournalRecordV4::decode_canonical(&canonical).unwrap(),
            record
        );
        assert!(matches!(
            Stage6JournalRecordVersioned::decode_canonical(&canonical).unwrap(),
            Stage6JournalRecordVersioned::V4(_)
        ));

        let mut wrong_kind: Value = serde_json::from_slice(&canonical).unwrap();
        wrong_kind["record_kind"] = Value::from("request_accepted");
        assert!(Stage6JournalRecordVersioned::decode_canonical(
            &serde_json::to_vec(&wrong_kind).unwrap()
        )
        .is_err());

        let mut malformed_suffix: Value = serde_json::from_slice(&canonical).unwrap();
        malformed_suffix["candidate_or_last_eligible_m10"]["redis_id"] =
            Value::from("1789387800000");
        assert!(Stage6JournalRecordVersioned::decode_canonical(
            &serde_json::to_vec(&malformed_suffix).unwrap()
        )
        .is_err());

        for noncanonical_redis_id in [
            "01789387800000-0",
            "1789387800000-00",
            "1789387800000-18446744073709551616",
        ] {
            let mut malformed_id: Value = serde_json::from_slice(&canonical).unwrap();
            malformed_id["redis_stream_id"] = Value::from(noncanonical_redis_id);
            assert!(Stage6JournalRecordVersioned::decode_canonical(
                &serde_json::to_vec(&malformed_id).unwrap()
            )
            .is_err());
        }

        let mut unknown: Value = serde_json::from_slice(&canonical).unwrap();
        unknown["schema_version"] = Value::from(99);
        assert_eq!(
            Stage6JournalRecordVersioned::decode_canonical(&serde_json::to_vec(&unknown).unwrap())
                .unwrap_err(),
            Stage6ReconciliationV2Error::UnsupportedSchema(99)
        );
    }

    #[test]
    fn market_v4_is_the_exact_request_predecessor_of_dispatch() {
        let (identity, accepted, _) = place_fixture();
        let accepted_payload_sha256 = accepted.canonical_payload_sha256().clone();
        let candidate = crate::stage8b_p1e_test_market_binding_candidate(
            "d".repeat(64),
            identity.strategy_request_id().to_string(),
            "f".repeat(64),
        );
        let binding = Stage6JournalRecordV4::from_stage8b_p1e_candidate(
            &candidate,
            Stage6LifecycleSequence::new(2).unwrap(),
            accepted.journal_record_id().clone(),
            7,
            DateTime::parse_from_rfc3339("2026-09-14T12:30:00.000000Z")
                .unwrap()
                .with_timezone(&Utc),
        )
        .unwrap();
        let dispatch = Stage6JournalRecordV1::dispatch_attempt_recorded(
            identity.clone(),
            1,
            accepted_payload_sha256.clone(),
            Stage6LifecycleSequence::new(3).unwrap(),
            Some(binding.journal_record_id().clone()),
            digest('2'),
        )
        .unwrap();
        let replay = Stage6MixedReplayEngineV2::replay(&[
            Stage6JournalRecordVersioned::V1(accepted),
            Stage6JournalRecordVersioned::V4(binding.clone()),
            Stage6JournalRecordVersioned::V1(dispatch.clone()),
        ])
        .unwrap();
        let request = &replay.requests()[0];
        assert_eq!(request.last_unique_sequence(), 3);
        assert_eq!(
            request.last_unique_record_id(),
            dispatch.journal_record_id()
        );
        assert_eq!(request.dispatch_attempt_count(), 1);
        assert_eq!(
            request.dispatch_safety_state(),
            crate::Stage6DispatchSafetyStateV1::ReconciliationRequired
        );

        let wrong_predecessor = Stage6JournalRecordV1::dispatch_attempt_recorded(
            identity,
            1,
            accepted_payload_sha256,
            Stage6LifecycleSequence::new(3).unwrap(),
            Some(binding.previous_record_id().clone()),
            digest('2'),
        )
        .unwrap();
        assert!(Stage6MixedReplayEngineV2::replay(&[
            Stage6JournalRecordVersioned::V1(place_fixture().1),
            Stage6JournalRecordVersioned::V4(binding),
            Stage6JournalRecordVersioned::V1(wrong_predecessor),
        ])
        .is_err());
    }

    #[test]
    fn complete_v2_fact_retains_absent_optional_broker_ids() {
        let (identity, _, attempt) = place_fixture();
        let mut payload = payload(
            &identity,
            Stage6ReconciliationEndpointKindV2::Place,
            Stage6ReconciliationTransitionKindV2::Exact {
                lifecycle: Stage6ReconciliationLifecycleV2::Working,
            },
            Stage6ExactLookupEvidenceV2::NotAttempted,
            None,
            vec![trade_fact(&identity, None)],
            Stage6ReconciliationFillEffectV2::Partial {
                filled_qty: Decimal::new(5, 1),
            },
            Vec::new(),
        );
        let order = payload.broker_order_fact.as_mut().unwrap();
        order.status = OrderStatus::PartiallyFilled;
        order.filled_qty = Decimal::new(5, 1);
        order.remaining_qty = Some(Decimal::new(5, 1));
        let v2 = Stage6JournalRecordV2::build_for_test(
            identity,
            Stage6LifecycleSequence::new(3).unwrap(),
            attempt.journal_record_id().clone(),
            payload,
            digest('c'),
        );
        let decoded = Stage6JournalRecordV2::decode_canonical(&v2.encode_canonical()).unwrap();
        assert!(decoded
            .payload()
            .broker_order_fact()
            .unwrap()
            .broker_order_id()
            .is_none());
        assert!(decoded.payload().material_trade_facts()[0]
            .broker_order_id()
            .is_none());
        assert_eq!(decoded.payload().suffix_manifest().entries().len(), 0);
    }

    #[test]
    fn mixed_replay_exposes_empty_partial_and_complete_batches() {
        let (identity, accepted, attempt) = place_fixture();
        let (empty, _) = v2_with_suffix(&identity, &attempt, 0);
        let empty_snapshot = Stage6MixedReplayEngineV2::replay(&[
            Stage6JournalRecordVersioned::V1(accepted.clone()),
            Stage6JournalRecordVersioned::V1(attempt.clone()),
            Stage6JournalRecordVersioned::V2(empty),
        ])
        .unwrap();
        assert_eq!(
            empty_snapshot.reconciliation_batches()[0].completion(),
            Stage6ReconciliationBatchCompletionV2::Complete
        );

        let (single_v2, single_suffix) = v2_with_suffix(&identity, &attempt, 1);
        let single_complete = Stage6MixedReplayEngineV2::replay(&[
            Stage6JournalRecordVersioned::V1(accepted.clone()),
            Stage6JournalRecordVersioned::V1(attempt.clone()),
            Stage6JournalRecordVersioned::V2(single_v2),
            Stage6JournalRecordVersioned::V1(single_suffix[0].clone()),
        ])
        .unwrap();
        assert_eq!(
            single_complete.reconciliation_batches()[0].completion(),
            Stage6ReconciliationBatchCompletionV2::Complete
        );

        let (v2, suffix) = v2_with_suffix(&identity, &attempt, 2);
        let partial = Stage6MixedReplayEngineV2::replay(&[
            Stage6JournalRecordVersioned::V1(accepted.clone()),
            Stage6JournalRecordVersioned::V1(attempt.clone()),
            Stage6JournalRecordVersioned::V2(v2.clone()),
            Stage6JournalRecordVersioned::V1(suffix[0].clone()),
        ])
        .unwrap();
        let batch = &partial.reconciliation_batches()[0];
        assert_eq!(
            batch.completion(),
            Stage6ReconciliationBatchCompletionV2::Incomplete
        );
        assert_eq!(batch.verified_suffix_prefix_length(), 1);
        assert_eq!(batch.missing_suffix_entries().len(), 1);
        assert_eq!(batch.transition_record(), &v2);

        let complete = Stage6MixedReplayEngineV2::replay(&[
            Stage6JournalRecordVersioned::V1(accepted),
            Stage6JournalRecordVersioned::V1(attempt),
            Stage6JournalRecordVersioned::V2(v2),
            Stage6JournalRecordVersioned::V1(suffix[0].clone()),
            Stage6JournalRecordVersioned::V1(suffix[1].clone()),
        ])
        .unwrap();
        assert_eq!(
            complete.reconciliation_batches()[0].completion(),
            Stage6ReconciliationBatchCompletionV2::Complete
        );
        assert_eq!(
            complete.requests()[0].final_disposition(),
            Some(Stage6RequestFinalDispositionV1::Completed)
        );
    }

    #[test]
    fn mixed_replay_rejects_unexpected_suffix_and_second_transition() {
        let (identity, accepted, attempt) = place_fixture();
        let (v2, suffix) = v2_with_suffix(&identity, &attempt, 2);
        let wrong = Stage6JournalRecordV1::request_finalized(
            identity.clone(),
            Stage6RequestFinalDispositionV1::Completed,
            Stage6LifecycleSequence::new(4).unwrap(),
            Some(v2.journal_record_id().clone()),
            digest('d'),
        )
        .unwrap();
        assert_eq!(
            Stage6MixedReplayEngineV2::replay(&[
                Stage6JournalRecordVersioned::V1(accepted.clone()),
                Stage6JournalRecordVersioned::V1(attempt.clone()),
                Stage6JournalRecordVersioned::V2(v2.clone()),
                Stage6JournalRecordVersioned::V1(wrong),
            ])
            .unwrap_err(),
            Stage6ReconciliationV2Error::UnexpectedSuffixRecord
        );

        let mut second_payload = v2.payload().clone();
        second_payload.deterministic_suffix_manifest.entries.clear();
        let second = Stage6JournalRecordV2::build_for_test(
            identity,
            Stage6LifecycleSequence::new(6).unwrap(),
            suffix[1].journal_record_id().clone(),
            second_payload,
            digest('e'),
        );
        assert_eq!(
            Stage6MixedReplayEngineV2::replay(&[
                Stage6JournalRecordVersioned::V1(accepted),
                Stage6JournalRecordVersioned::V1(attempt),
                Stage6JournalRecordVersioned::V2(v2),
                Stage6JournalRecordVersioned::V1(suffix[0].clone()),
                Stage6JournalRecordVersioned::V1(suffix[1].clone()),
                Stage6JournalRecordVersioned::V2(second),
            ])
            .unwrap_err(),
            Stage6ReconciliationV2Error::V2AfterFinalization
        );
    }

    #[test]
    fn same_stable_transition_key_with_different_v2_payload_fails_closed() {
        let (identity, accepted, attempt) = place_fixture();
        let (first, _) = v2_with_suffix(&identity, &attempt, 0);
        let mut second_payload = first.payload().clone();
        second_payload.transition_kind =
            Stage6ReconciliationTransitionKindV2::ReconciliationConflictHold;
        let second = Stage6JournalRecordV2::build_for_test(
            identity,
            Stage6LifecycleSequence::new(4).unwrap(),
            first.journal_record_id().clone(),
            second_payload,
            digest('d'),
        );
        assert_eq!(
            Stage6MixedReplayEngineV2::replay(&[
                Stage6JournalRecordVersioned::V1(accepted),
                Stage6JournalRecordVersioned::V1(attempt),
                Stage6JournalRecordVersioned::V2(first),
                Stage6JournalRecordVersioned::V2(second),
            ])
            .unwrap_err(),
            Stage6ReconciliationV2Error::PendingBatchConflict
        );
    }

    #[test]
    fn exact_duplicate_v2_is_idempotent_but_suffix_source_or_causality_drift_fails() {
        let (identity, accepted, attempt) = place_fixture();
        let (v2, suffix) = v2_with_suffix(&identity, &attempt, 1);
        let duplicate = Stage6MixedReplayEngineV2::replay(&[
            Stage6JournalRecordVersioned::V1(accepted.clone()),
            Stage6JournalRecordVersioned::V1(attempt.clone()),
            Stage6JournalRecordVersioned::V2(v2.clone()),
            Stage6JournalRecordVersioned::V2(v2.clone()),
        ])
        .unwrap();
        assert_eq!(duplicate.reconciliation_batches().len(), 1);
        assert_eq!(
            duplicate.reconciliation_batches()[0].completion(),
            Stage6ReconciliationBatchCompletionV2::Incomplete
        );

        let source_drift = Stage6JournalRecordV1::broker_order_observed(
            identity.clone(),
            BrokerOrderId::new("ORDER-1"),
            Stage6LifecycleSequence::new(4).unwrap(),
            Some(v2.journal_record_id().clone()),
            digest('d'),
        )
        .unwrap();
        assert_eq!(
            Stage6MixedReplayEngineV2::replay(&[
                Stage6JournalRecordVersioned::V1(accepted.clone()),
                Stage6JournalRecordVersioned::V1(attempt.clone()),
                Stage6JournalRecordVersioned::V2(v2.clone()),
                Stage6JournalRecordVersioned::V1(source_drift),
            ])
            .unwrap_err(),
            Stage6ReconciliationV2Error::UnexpectedSuffixRecord
        );

        let causal_drift = Stage6JournalRecordV1::broker_order_observed(
            identity,
            BrokerOrderId::new("ORDER-1"),
            Stage6LifecycleSequence::new(4).unwrap(),
            Some(attempt.journal_record_id().clone()),
            digest('a'),
        )
        .unwrap();
        assert_eq!(
            Stage6MixedReplayEngineV2::replay(&[
                Stage6JournalRecordVersioned::V1(accepted),
                Stage6JournalRecordVersioned::V1(attempt),
                Stage6JournalRecordVersioned::V2(v2),
                Stage6JournalRecordVersioned::V1(causal_drift),
            ])
            .unwrap_err(),
            Stage6ReconciliationV2Error::UnexpectedSuffixRecord
        );
        assert_eq!(suffix.len(), 1);
    }

    #[test]
    fn exact_lookup_durable_binding_mismatch_fails_closed() {
        let (identity, _, attempt) = place_fixture();
        let mut lookup = lookup_variants(&identity).remove(2).1;
        if let Stage6ExactLookupEvidenceV2::DocumentedNotFound {
            durable_request_binding_sha256,
            ..
        } = &mut lookup
        {
            *durable_request_binding_sha256 = digest('f');
        }
        let candidate = payload(
            &identity,
            Stage6ReconciliationEndpointKindV2::Place,
            Stage6ReconciliationTransitionKindV2::ReconciliationStillUnknownHold,
            lookup,
            Some("ORDER-1"),
            Vec::new(),
            Stage6ReconciliationFillEffectV2::Zero,
            Vec::new(),
        );
        let canonical_payload_sha256 =
            Stage6Sha256Digest::of(&serde_json::to_vec(&candidate).unwrap());
        let value = Stage6JournalRecordV2 {
            schema_version: STAGE6_DURABLE_RECORD_SCHEMA_VERSION_V2,
            journal_record_id: Stage6JournalRecordId::derive(
                identity.strategy_request_id(),
                Stage6LifecycleSequence::new(3).unwrap(),
            ),
            lifecycle_sequence: Stage6LifecycleSequence::new(3).unwrap(),
            previous_record_id: Some(attempt.journal_record_id().clone()),
            causal_parent_id: Some(attempt.journal_record_id().clone()),
            durable_request_identity: identity,
            event_kind: Stage6JournalEventKindV2::ReconciliationTransitionApplied,
            payload: candidate,
            canonical_payload_sha256,
            source_evidence_sha256: digest('c'),
        };
        assert_eq!(
            Stage6JournalRecordV2::decode_canonical(&value.encode_canonical()).unwrap_err(),
            Stage6ReconciliationV2Error::InvalidLookupEvidence
        );
    }

    #[test]
    fn versioned_framed_reader_and_backend_read_mixed_with_v1_projection() {
        let (identity, accepted, attempt) = place_fixture();
        let (v2, _) = v2_with_suffix(&identity, &attempt, 0);
        let records = vec![
            Stage6JournalRecordVersioned::V1(accepted),
            Stage6JournalRecordVersioned::V1(attempt),
            Stage6JournalRecordVersioned::V2(v2),
        ];
        let bytes = crate::stage6_journal_backend::frame_versioned_records_for_test(&records);
        let decoded = Stage6VersionedJournalReader::read_framed_bytes(&bytes).unwrap();
        assert_eq!(decoded.len(), 3);
        assert!(matches!(decoded[2], Stage6JournalRecordVersioned::V2(_)));
        let backend = crate::Stage6MemoryJournalBackend::from_framed_bytes(bytes).unwrap();
        assert_eq!(backend.versioned_records().len(), 3);
        assert_eq!(backend.records().len(), 2);
        assert!(matches!(
            backend.versioned_records()[2],
            Stage6JournalRecordVersioned::V2(_)
        ));
    }

    #[test]
    fn all_exact_lookup_variants_are_canonical_and_attempted_states_stay_distinct() {
        let (identity, _, attempt) = place_fixture();
        let order = order_fact(&identity, Some("ORDER-1"));
        let attempted = vec![
            Stage6ExactLookupEvidenceV2::Succeeded {
                account_id: identity.account_id().clone(),
                queried_broker_order_id: BrokerOrderId::new("ORDER-1"),
                durable_request_binding_sha256: digest('4'),
                request_started_at: now(),
                response_received_at: now(),
                exact_order_observation_v2: Stage6ExactOrderObservationV2 {
                    order,
                    observation_binding_sha256: digest('d'),
                },
            },
            Stage6ExactLookupEvidenceV2::DocumentedNotFound {
                account_id: identity.account_id().clone(),
                queried_broker_order_id: BrokerOrderId::new("ORDER-1"),
                durable_request_binding_sha256: digest('4'),
                request_started_at: now(),
                response_received_at: now(),
                documented_status_category: "documented_not_found".into(),
            },
            Stage6ExactLookupEvidenceV2::Unavailable {
                account_id: identity.account_id().clone(),
                queried_broker_order_id: BrokerOrderId::new("ORDER-1"),
                durable_request_binding_sha256: digest('4'),
                request_started_at: now(),
                response_received_at: now(),
                failure_category: "timeout".into(),
            },
            Stage6ExactLookupEvidenceV2::DecodeFailure {
                account_id: identity.account_id().clone(),
                queried_broker_order_id: BrokerOrderId::new("ORDER-1"),
                durable_request_binding_sha256: digest('4'),
                request_started_at: now(),
                response_received_at: now(),
                response_status_category: "success_2xx".into(),
                response_binding_sha256: digest('e'),
            },
            Stage6ExactLookupEvidenceV2::Stale {
                account_id: identity.account_id().clone(),
                queried_broker_order_id: BrokerOrderId::new("ORDER-1"),
                durable_request_binding_sha256: digest('4'),
                request_started_at: now(),
                response_received_at: now(),
                stale_observation_binding_sha256: digest('f'),
            },
        ];
        for lookup in attempted {
            let record = Stage6JournalRecordV2::build_for_test(
                identity.clone(),
                Stage6LifecycleSequence::new(3).unwrap(),
                attempt.journal_record_id().clone(),
                payload(
                    &identity,
                    Stage6ReconciliationEndpointKindV2::Place,
                    Stage6ReconciliationTransitionKindV2::ReconciliationConflictHold,
                    lookup,
                    Some("ORDER-1"),
                    Vec::new(),
                    Stage6ReconciliationFillEffectV2::Zero,
                    Vec::new(),
                ),
                digest('c'),
            );
            let bytes = record.encode_canonical();
            assert_eq!(
                Stage6JournalRecordV2::decode_canonical(&bytes)
                    .unwrap()
                    .encode_canonical(),
                bytes
            );
            assert!(!String::from_utf8(bytes).unwrap().contains("not_attempted"));
        }
    }

    #[test]
    fn cancel_transition_uses_cancel_identity_without_place_projection() {
        let (identity, accepted, attempt) = cancel_fixture();
        let mut payload = payload(
            &identity,
            Stage6ReconciliationEndpointKindV2::Cancel,
            Stage6ReconciliationTransitionKindV2::Exact {
                lifecycle: Stage6ReconciliationLifecycleV2::Working,
            },
            Stage6ExactLookupEvidenceV2::NotAttempted,
            Some("ORDER-1"),
            Vec::new(),
            Stage6ReconciliationFillEffectV2::Zero,
            Vec::new(),
        );
        payload.broker_order_fact.as_mut().unwrap().client_order_id =
            identity.target_order_client_order_id().cloned();
        let v2 = Stage6JournalRecordV2::build_for_test(
            identity,
            Stage6LifecycleSequence::new(3).unwrap(),
            attempt.journal_record_id().clone(),
            payload,
            digest('c'),
        );
        assert!(Stage6MixedReplayEngineV2::replay(&[
            Stage6JournalRecordVersioned::V1(accepted),
            Stage6JournalRecordVersioned::V1(attempt),
            Stage6JournalRecordVersioned::V2(v2),
        ])
        .is_ok());
    }
}
