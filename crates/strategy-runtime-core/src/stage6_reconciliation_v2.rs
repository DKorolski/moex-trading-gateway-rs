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
    Stage6DurableActionKind, Stage6DurableIdentityError, Stage6DurableRequestIdentityV1,
    Stage6JournalEventKind, Stage6JournalRecordId, Stage6JournalRecordV1, Stage6LifecycleSequence,
    Stage6RecoveredRequestV1, Stage6ReplayError, Stage6Sha256Digest,
};
use broker_core::{
    BrokerAccountId, BrokerOrderId, BrokerOrderLifecycle, BrokerTradeId, ClientOrderId,
    InstrumentId, OrderSide, OrderStatus, OrderType, Price, Quantity, TimeInForce,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const STAGE6_DURABLE_RECORD_SCHEMA_VERSION_V2: u16 = 2;
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
    fn matches(&self, record: &Stage6JournalRecordV1) -> bool {
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
    pub fn broker_order_fact(&self) -> Option<&Stage6BrokerOrderFactV2> {
        self.broker_order_fact.as_ref()
    }
    pub fn material_trade_facts(&self) -> &[Stage6MaterialTradeFactV2] {
        &self.material_trade_facts
    }
    pub fn suffix_manifest(&self) -> &Stage6SuffixManifestV2 {
        &self.deterministic_suffix_manifest
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
            let lifecycle_matches = match lifecycle {
                Stage6ReconciliationLifecycleV2::Working => {
                    order.lifecycle == BrokerOrderLifecycle::Active
                }
                Stage6ReconciliationLifecycleV2::TerminalFilled => {
                    order.status == OrderStatus::Filled
                }
                Stage6ReconciliationLifecycleV2::TerminalRejected => {
                    order.status == OrderStatus::Rejected
                }
                Stage6ReconciliationLifecycleV2::TerminalCancelled => {
                    order.status == OrderStatus::Canceled
                }
                Stage6ReconciliationLifecycleV2::TerminalExpired => {
                    order.status == OrderStatus::Expired
                }
            };
            let fill_matches = match self.fill_effect {
                Stage6ReconciliationFillEffectV2::Zero => order.filled_qty == Quantity::ZERO,
                Stage6ReconciliationFillEffectV2::Partial { filled_qty } => {
                    filled_qty == order.filled_qty
                        && filled_qty > Quantity::ZERO
                        && filled_qty < order.qty
                }
                Stage6ReconciliationFillEffectV2::Full { filled_qty } => {
                    filled_qty == order.filled_qty && filled_qty == order.qty
                }
            };
            let material_qty: Quantity = self
                .material_trade_facts
                .iter()
                .map(|trade| trade.qty)
                .sum();
            if !lifecycle_matches || !fill_matches || material_qty != order.filled_qty {
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

#[derive(Debug, Clone, PartialEq)]
// R2 freezes the exact V1(Stage6JournalRecordV1) | V2(Stage6JournalRecordV2)
// shape; this read-only replay enum is not a high-volume queue element.
#[allow(clippy::large_enum_variant)]
pub enum Stage6JournalRecordVersioned {
    V1(Stage6JournalRecordV1),
    V2(Stage6JournalRecordV2),
}

impl Stage6JournalRecordVersioned {
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Stage6ReconciliationV2Error> {
        match probe_schema_version(bytes)? {
            1 => Stage6JournalRecordV1::decode_canonical(bytes)
                .map(Self::V1)
                .map_err(map_v1_error),
            2 => Stage6JournalRecordV2::decode_canonical(bytes).map(Self::V2),
            value => Err(Stage6ReconciliationV2Error::UnsupportedSchema(value)),
        }
    }
    pub fn encode_canonical(&self) -> Vec<u8> {
        match self {
            Self::V1(value) => value.encode_canonical(),
            Self::V2(value) => value.encode_canonical(),
        }
    }
    pub fn journal_record_id(&self) -> &Stage6JournalRecordId {
        match self {
            Self::V1(value) => value.journal_record_id(),
            Self::V2(value) => value.journal_record_id(),
        }
    }
    pub fn lifecycle_sequence(&self) -> Stage6LifecycleSequence {
        match self {
            Self::V1(value) => value.lifecycle_sequence(),
            Self::V2(value) => value.lifecycle_sequence(),
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
}

impl Stage6MixedReplaySnapshotV2 {
    pub fn requests(&self) -> &[Stage6RecoveredRequestV1] {
        &self.requests
    }
    pub fn reconciliation_batches(&self) -> &[Stage6PendingReconciliationBatchV2] {
        &self.reconciliation_batches
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
        let mut requests = BTreeMap::<String, MixedWorkingRequest>::new();
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
                                    if !entry.matches(v1) {
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
            }
            seen.insert(key, canonical);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Stage6DurableCommandSnapshotV1, Stage6JournalRecordV1, Stage6RequestFinalDispositionV1,
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
            client_order_id: Some(identity.durable_client_order_id().clone()),
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
        unknown["schema_version"] = Value::from(3);
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
        unknown["schema_version"] = Value::from(3);
        assert_eq!(
            Stage6JournalRecordVersioned::decode_canonical(&serde_json::to_vec(&unknown).unwrap())
                .unwrap_err(),
            Stage6ReconciliationV2Error::UnsupportedSchema(3)
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
    fn versioned_framed_reader_reads_mixed_while_legacy_reader_fails_closed() {
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
        assert!(crate::Stage6MemoryJournalBackend::from_framed_bytes(bytes).is_err());
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
