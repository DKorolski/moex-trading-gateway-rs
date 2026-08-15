//! Stage 6C pure deterministic crash/replay state machine.
//!
//! This module consumes only records already admitted by the Stage 6A canonical
//! authority and Stage 6B storage scanner. It has no filesystem, Redis, FINAM,
//! broker-dispatch, runtime callback, scheduler or live-execution authority.

use crate::stage6_durable_identity::Stage6JournalPayloadV1;
use crate::{
    Stage6CancelOutcomeV1, Stage6DurableActionKind, Stage6DurableRequestIdentityV1,
    Stage6JournalEventKind, Stage6JournalRecordId, Stage6JournalRecordV1,
    Stage6ReconciliationDispositionV1, Stage6RequestFinalDispositionV1, Stage6Sha256Digest,
};
use broker_core::{BrokerOrderId, BrokerTradeId, ClientOrderId, StrategyRequestId};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const STAGE6_REPLAY_SCHEMA_VERSION: u16 = 1;
const REPLAY_FINGERPRINT_DOMAIN: &[u8] = b"stage6-replay-snapshot-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage6DispatchSafetyStateV1 {
    ReadyForFirstDispatch,
    ReconciliationRequired,
    RetryEligibleSameIdentity,
    DispatchForbidden,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage6ReplayError {
    SequenceStartInvalid,
    SequenceGap,
    PreviousRecordMismatch,
    IdentityDrift,
    ConflictingReplay,
    CausalParentMissing,
    DispatchAttemptInvalid,
    BlindRedispatchBlocked,
    BrokerOrderConflict,
    BrokerTradeConflict,
    CancelTargetConflict,
    CancelOutcomeConflict,
    InvalidActionEvent,
    EventAfterFinalization,
    InvalidTransition,
}

impl std::fmt::Display for Stage6ReplayError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::SequenceStartInvalid => "invalid first Stage 6 replay sequence",
            Self::SequenceGap => "Stage 6 replay sequence gap or regression",
            Self::PreviousRecordMismatch => "Stage 6 previous record mismatch",
            Self::IdentityDrift => "Stage 6 durable request identity drift",
            Self::ConflictingReplay => "conflicting Stage 6 record replay",
            Self::CausalParentMissing => "Stage 6 causal parent is missing or future",
            Self::DispatchAttemptInvalid => "invalid Stage 6 dispatch attempt",
            Self::BlindRedispatchBlocked => "blind Stage 6 redispatch is blocked",
            Self::BrokerOrderConflict => "conflicting Stage 6 broker order identity",
            Self::BrokerTradeConflict => "conflicting Stage 6 broker trade identity",
            Self::CancelTargetConflict => "conflicting Stage 6 cancel target",
            Self::CancelOutcomeConflict => "conflicting Stage 6 cancel outcome",
            Self::InvalidActionEvent => "Stage 6 event is invalid for durable action",
            Self::EventAfterFinalization => "Stage 6 lifecycle event follows finalization",
            Self::InvalidTransition => "invalid Stage 6 replay transition",
        })
    }
}

impl std::error::Error for Stage6ReplayError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage6RecoveredRequestV1 {
    strategy_request_id: StrategyRequestId,
    durable_client_order_id: ClientOrderId,
    action: Stage6DurableActionKind,
    last_unique_sequence: u64,
    last_unique_record_id: Stage6JournalRecordId,
    dispatch_safety_state: Stage6DispatchSafetyStateV1,
    dispatch_attempt_count: u32,
    known_broker_order_id: Option<BrokerOrderId>,
    observed_broker_trade_ids: Vec<BrokerTradeId>,
    cancel_outcome: Option<Stage6CancelOutcomeV1>,
    final_disposition: Option<Stage6RequestFinalDispositionV1>,
    conflict_observed: bool,
}

impl Stage6RecoveredRequestV1 {
    pub fn strategy_request_id(&self) -> StrategyRequestId {
        self.strategy_request_id
    }
    pub fn durable_client_order_id(&self) -> &ClientOrderId {
        &self.durable_client_order_id
    }
    pub fn action(&self) -> Stage6DurableActionKind {
        self.action
    }
    pub fn last_unique_sequence(&self) -> u64 {
        self.last_unique_sequence
    }
    pub fn last_unique_record_id(&self) -> &Stage6JournalRecordId {
        &self.last_unique_record_id
    }
    pub fn dispatch_safety_state(&self) -> Stage6DispatchSafetyStateV1 {
        self.dispatch_safety_state
    }
    pub fn dispatch_attempt_count(&self) -> u32 {
        self.dispatch_attempt_count
    }
    pub fn known_broker_order_id(&self) -> Option<&BrokerOrderId> {
        self.known_broker_order_id.as_ref()
    }
    pub fn observed_broker_trade_ids(&self) -> &[BrokerTradeId] {
        &self.observed_broker_trade_ids
    }
    pub fn cancel_outcome(&self) -> Option<Stage6CancelOutcomeV1> {
        self.cancel_outcome
    }
    pub fn final_disposition(&self) -> Option<Stage6RequestFinalDispositionV1> {
        self.final_disposition
    }
    pub fn conflict_observed(&self) -> bool {
        self.conflict_observed
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage6ReplaySnapshotV1 {
    replay_schema_version: u16,
    requests: Vec<Stage6RecoveredRequestV1>,
    semantic_fingerprint_sha256: Stage6Sha256Digest,
}

impl Stage6ReplaySnapshotV1 {
    pub fn requests(&self) -> &[Stage6RecoveredRequestV1] {
        &self.requests
    }
    pub fn request(&self, request_id: StrategyRequestId) -> Option<&Stage6RecoveredRequestV1> {
        self.requests
            .iter()
            .find(|request| request.strategy_request_id == request_id)
    }
    pub fn semantic_fingerprint_sha256(&self) -> &Stage6Sha256Digest {
        &self.semantic_fingerprint_sha256
    }
}

#[derive(Debug, Default)]
pub struct Stage6ReplayEngineV1;

impl Stage6ReplayEngineV1 {
    pub fn replay(
        records: &[Stage6JournalRecordV1],
    ) -> Result<Stage6ReplaySnapshotV1, Stage6ReplayError> {
        let mut seen_records = BTreeMap::<String, Vec<u8>>::new();
        let mut requests = BTreeMap::<String, WorkingRequest>::new();

        for record in records {
            let record_key = record.journal_record_id().as_str().to_string();
            let canonical = record.encode_canonical();
            if let Some(previous) = seen_records.get(&record_key) {
                if previous == &canonical {
                    continue;
                }
                return Err(Stage6ReplayError::ConflictingReplay);
            }
            if record
                .causal_parent_id()
                .is_some_and(|parent| !seen_records.contains_key(parent.as_str()))
            {
                return Err(Stage6ReplayError::CausalParentMissing);
            }

            let identity = record.durable_request_identity();
            let request_key = identity.strategy_request_id().to_string();
            match requests.get_mut(&request_key) {
                None => {
                    let state = WorkingRequest::from_first(record)?;
                    requests.insert(request_key, state);
                }
                Some(state) => state.apply(record)?,
            }
            seen_records.insert(record_key, canonical);
        }

        let requests = requests
            .into_values()
            .map(WorkingRequest::into_recovered)
            .collect::<Vec<_>>();
        let semantic_fingerprint_sha256 = replay_fingerprint(&requests);
        Ok(Stage6ReplaySnapshotV1 {
            replay_schema_version: STAGE6_REPLAY_SCHEMA_VERSION,
            requests,
            semantic_fingerprint_sha256,
        })
    }
}

#[derive(Debug)]
pub(crate) struct WorkingRequest {
    identity: Stage6DurableRequestIdentityV1,
    accepted_payload_sha256: Stage6Sha256Digest,
    last_sequence: u64,
    last_record_id: Stage6JournalRecordId,
    dispatch_safety_state: Stage6DispatchSafetyStateV1,
    dispatch_attempt_count: u32,
    known_broker_order_id: Option<BrokerOrderId>,
    trades: BTreeMap<String, BrokerOrderId>,
    cancel_outcome: Option<Stage6CancelOutcomeV1>,
    final_disposition: Option<Stage6RequestFinalDispositionV1>,
    conflict_observed: bool,
}

impl WorkingRequest {
    pub(crate) fn from_first(record: &Stage6JournalRecordV1) -> Result<Self, Stage6ReplayError> {
        if record.event_kind() != Stage6JournalEventKind::RequestAccepted
            || record.lifecycle_sequence().get() != 1
            || record.previous_record_id().is_some()
        {
            return Err(Stage6ReplayError::SequenceStartInvalid);
        }
        Ok(Self {
            identity: record.durable_request_identity().clone(),
            accepted_payload_sha256: record.canonical_payload_sha256().clone(),
            last_sequence: 1,
            last_record_id: record.journal_record_id().clone(),
            dispatch_safety_state: Stage6DispatchSafetyStateV1::ReadyForFirstDispatch,
            dispatch_attempt_count: 0,
            known_broker_order_id: None,
            trades: BTreeMap::new(),
            cancel_outcome: None,
            final_disposition: None,
            conflict_observed: false,
        })
    }

    pub(crate) fn apply(
        &mut self,
        record: &Stage6JournalRecordV1,
    ) -> Result<(), Stage6ReplayError> {
        self.validate_identity(record.durable_request_identity())?;
        if self.final_disposition.is_some() {
            return Err(Stage6ReplayError::EventAfterFinalization);
        }
        if record.lifecycle_sequence().get() != self.last_sequence + 1 {
            return Err(Stage6ReplayError::SequenceGap);
        }
        if record.previous_record_id() != Some(&self.last_record_id) {
            return Err(Stage6ReplayError::PreviousRecordMismatch);
        }
        validate_event_for_action(self.identity.action(), record.payload())?;

        match record.payload() {
            Stage6JournalPayloadV1::RequestAccepted { .. } => {
                return Err(Stage6ReplayError::InvalidTransition);
            }
            Stage6JournalPayloadV1::DispatchAttemptRecorded {
                attempt_ordinal,
                accepted_request_payload_sha256,
            } => self.apply_dispatch_attempt(*attempt_ordinal, accepted_request_payload_sha256)?,
            Stage6JournalPayloadV1::BrokerOrderObserved { broker_order_id } => {
                self.require_dispatch_attempt()?;
                self.establish_broker_order(broker_order_id)?;
                self.dispatch_safety_state = Stage6DispatchSafetyStateV1::DispatchForbidden;
            }
            Stage6JournalPayloadV1::BrokerTradeObserved {
                broker_trade_id,
                broker_order_id,
            } => {
                self.require_dispatch_attempt()?;
                if let Some(existing) = self.trades.get(broker_trade_id.as_str()) {
                    if existing != broker_order_id {
                        return Err(Stage6ReplayError::BrokerTradeConflict);
                    }
                } else {
                    self.trades.insert(
                        broker_trade_id.as_str().to_string(),
                        broker_order_id.clone(),
                    );
                }
                self.establish_broker_order(broker_order_id)?;
                self.dispatch_safety_state = Stage6DispatchSafetyStateV1::DispatchForbidden;
            }
            Stage6JournalPayloadV1::ReconciliationObserved { disposition } => {
                self.apply_reconciliation(disposition)?;
            }
            Stage6JournalPayloadV1::CancelOutcomeObserved {
                target_broker_order_id,
                outcome,
            } => {
                if self.identity.action() != Stage6DurableActionKind::Cancel
                    || self.identity.target_broker_order_id() != Some(target_broker_order_id)
                {
                    return Err(Stage6ReplayError::CancelTargetConflict);
                }
                self.require_dispatch_attempt()?;
                if self.cancel_outcome.is_some() {
                    return Err(Stage6ReplayError::CancelOutcomeConflict);
                }
                self.cancel_outcome = Some(*outcome);
                self.dispatch_safety_state = Stage6DispatchSafetyStateV1::DispatchForbidden;
            }
            Stage6JournalPayloadV1::RequestFinalized { disposition } => {
                self.final_disposition = Some(*disposition);
                self.dispatch_safety_state = Stage6DispatchSafetyStateV1::DispatchForbidden;
            }
            Stage6JournalPayloadV1::ConflictObserved { .. } => {
                self.conflict_observed = true;
                self.dispatch_safety_state = Stage6DispatchSafetyStateV1::DispatchForbidden;
            }
            Stage6JournalPayloadV1::Marker => {
                return Err(Stage6ReplayError::InvalidTransition);
            }
        }
        self.last_sequence = record.lifecycle_sequence().get();
        self.last_record_id = record.journal_record_id().clone();
        Ok(())
    }

    pub(crate) fn advance_causal_only(
        &mut self,
        identity: &Stage6DurableRequestIdentityV1,
        sequence: crate::Stage6LifecycleSequence,
        previous_record_id: Option<&Stage6JournalRecordId>,
        record_id: Stage6JournalRecordId,
    ) -> Result<(), Stage6ReplayError> {
        self.validate_identity(identity)?;
        if self.final_disposition.is_some() {
            return Err(Stage6ReplayError::EventAfterFinalization);
        }
        if sequence.get() != self.last_sequence + 1 {
            return Err(Stage6ReplayError::SequenceGap);
        }
        if previous_record_id != Some(&self.last_record_id) {
            return Err(Stage6ReplayError::PreviousRecordMismatch);
        }
        self.last_sequence = sequence.get();
        self.last_record_id = record_id;
        Ok(())
    }

    pub(crate) fn is_finalized(&self) -> bool {
        self.final_disposition.is_some()
    }

    fn validate_identity(
        &self,
        observed: &Stage6DurableRequestIdentityV1,
    ) -> Result<(), Stage6ReplayError> {
        if self.identity.action() == Stage6DurableActionKind::Cancel
            && (self.identity.target_broker_order_id() != observed.target_broker_order_id()
                || self.identity.target_order_client_order_id()
                    != observed.target_order_client_order_id())
        {
            return Err(Stage6ReplayError::CancelTargetConflict);
        }
        if &self.identity != observed {
            return Err(Stage6ReplayError::IdentityDrift);
        }
        Ok(())
    }

    fn apply_dispatch_attempt(
        &mut self,
        attempt_ordinal: u32,
        accepted_request_payload_sha256: &Stage6Sha256Digest,
    ) -> Result<(), Stage6ReplayError> {
        if accepted_request_payload_sha256 != &self.accepted_payload_sha256
            || attempt_ordinal != self.dispatch_attempt_count + 1
        {
            return Err(Stage6ReplayError::DispatchAttemptInvalid);
        }
        match self.dispatch_safety_state {
            Stage6DispatchSafetyStateV1::ReadyForFirstDispatch
                if self.dispatch_attempt_count == 0 && attempt_ordinal == 1 => {}
            Stage6DispatchSafetyStateV1::RetryEligibleSameIdentity
                if self.identity.action() == Stage6DurableActionKind::Place => {}
            Stage6DispatchSafetyStateV1::RetryEligibleSameIdentity => {
                return Err(Stage6ReplayError::InvalidActionEvent);
            }
            Stage6DispatchSafetyStateV1::ReconciliationRequired => {
                return Err(Stage6ReplayError::BlindRedispatchBlocked);
            }
            Stage6DispatchSafetyStateV1::DispatchForbidden
            | Stage6DispatchSafetyStateV1::ReadyForFirstDispatch => {
                return Err(Stage6ReplayError::DispatchAttemptInvalid);
            }
        }
        self.dispatch_attempt_count = attempt_ordinal;
        self.dispatch_safety_state = Stage6DispatchSafetyStateV1::ReconciliationRequired;
        Ok(())
    }

    fn apply_reconciliation(
        &mut self,
        disposition: &Stage6ReconciliationDispositionV1,
    ) -> Result<(), Stage6ReplayError> {
        self.require_dispatch_attempt()?;
        match disposition {
            Stage6ReconciliationDispositionV1::NoBrokerOrderFound => {
                if self.identity.action() != Stage6DurableActionKind::Place {
                    return Err(Stage6ReplayError::InvalidActionEvent);
                }
                if self.dispatch_safety_state != Stage6DispatchSafetyStateV1::ReconciliationRequired
                    || self.known_broker_order_id.is_some()
                {
                    return Err(Stage6ReplayError::InvalidTransition);
                }
                self.dispatch_safety_state = Stage6DispatchSafetyStateV1::RetryEligibleSameIdentity;
            }
            Stage6ReconciliationDispositionV1::BrokerOrderFound { broker_order_id } => {
                if self.identity.action() != Stage6DurableActionKind::Place {
                    return Err(Stage6ReplayError::InvalidActionEvent);
                }
                self.establish_broker_order(broker_order_id)?;
                self.dispatch_safety_state = Stage6DispatchSafetyStateV1::DispatchForbidden;
            }
            Stage6ReconciliationDispositionV1::Inconclusive => {
                if self.dispatch_safety_state != Stage6DispatchSafetyStateV1::ReconciliationRequired
                    || self.known_broker_order_id.is_some()
                {
                    return Err(Stage6ReplayError::InvalidTransition);
                }
            }
        }
        Ok(())
    }

    fn require_dispatch_attempt(&self) -> Result<(), Stage6ReplayError> {
        if self.dispatch_attempt_count == 0 {
            Err(Stage6ReplayError::InvalidTransition)
        } else {
            Ok(())
        }
    }

    fn establish_broker_order(
        &mut self,
        broker_order_id: &BrokerOrderId,
    ) -> Result<(), Stage6ReplayError> {
        if self
            .known_broker_order_id
            .as_ref()
            .is_some_and(|known| known != broker_order_id)
        {
            return Err(Stage6ReplayError::BrokerOrderConflict);
        }
        self.known_broker_order_id = Some(broker_order_id.clone());
        Ok(())
    }

    pub(crate) fn into_recovered(self) -> Stage6RecoveredRequestV1 {
        Stage6RecoveredRequestV1 {
            strategy_request_id: self.identity.strategy_request_id(),
            durable_client_order_id: self.identity.durable_client_order_id().clone(),
            action: self.identity.action(),
            last_unique_sequence: self.last_sequence,
            last_unique_record_id: self.last_record_id,
            dispatch_safety_state: self.dispatch_safety_state,
            dispatch_attempt_count: self.dispatch_attempt_count,
            known_broker_order_id: self.known_broker_order_id,
            observed_broker_trade_ids: self.trades.into_keys().map(BrokerTradeId::new).collect(),
            cancel_outcome: self.cancel_outcome,
            final_disposition: self.final_disposition,
            conflict_observed: self.conflict_observed,
        }
    }
}

fn validate_event_for_action(
    action: Stage6DurableActionKind,
    payload: &Stage6JournalPayloadV1,
) -> Result<(), Stage6ReplayError> {
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
        Err(Stage6ReplayError::InvalidActionEvent)
    }
}

#[derive(Serialize)]
struct FingerprintInput<'a> {
    replay_schema_version: u16,
    requests: &'a [Stage6RecoveredRequestV1],
}

fn replay_fingerprint(requests: &[Stage6RecoveredRequestV1]) -> Stage6Sha256Digest {
    let input = FingerprintInput {
        replay_schema_version: STAGE6_REPLAY_SCHEMA_VERSION,
        requests,
    };
    let mut hasher = Sha256::new();
    hasher.update(REPLAY_FINGERPRINT_DOMAIN);
    hasher.update(serde_json::to_vec(&input).expect("fixed replay fingerprint input serializes"));
    let digest = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Stage6Sha256Digest::parse(digest).expect("SHA-256 digest is canonical")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Stage6DurableCommandSnapshotV1, Stage6JournalBackend, Stage6LifecycleSequence,
        Stage6MemoryJournalBackend,
    };
    use broker_core::{
        BrokerAccountId, CancelOrder, Exchange, HybridRuntimeAttribution, InstrumentId, Market,
        OrderSide, OrderType, PlaceOrder, TimeInForce,
    };
    use chrono::{TimeZone, Utc};
    use rust_decimal::Decimal;
    use uuid::Uuid;

    #[derive(Clone)]
    struct PlaceFixture {
        command: PlaceOrder,
        identity: Stage6DurableRequestIdentityV1,
    }

    #[derive(Clone)]
    struct CancelFixture {
        command: CancelOrder,
        identity: Stage6DurableRequestIdentityV1,
    }

    fn request(number: u128) -> StrategyRequestId {
        StrategyRequestId::from(Uuid::from_u128((number << 96) | number))
    }

    fn instrument(symbol: &str) -> InstrumentId {
        InstrumentId {
            symbol: symbol.into(),
            venue_symbol: Some(format!("{symbol}@RTSX")),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn attribution(cycle: &str, role: &str) -> HybridRuntimeAttribution {
        HybridRuntimeAttribution::parse_source_comment(format!(
            "HYB|sid=hybrid_imoexf|c={cycle}|o=BO|r={role}"
        ))
        .unwrap()
    }

    fn digest(byte: char) -> Stage6Sha256Digest {
        Stage6Sha256Digest::parse(byte.to_string().repeat(64)).unwrap()
    }

    fn decode_hex(value: &str) -> Vec<u8> {
        value
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let text = std::str::from_utf8(pair).unwrap();
                u8::from_str_radix(text, 16).unwrap()
            })
            .collect()
    }

    fn place_fixture(number: u128) -> PlaceFixture {
        place_fixture_with(number, "ACC_TEST_0001", "IMOEXF", "cycle0001")
    }

    fn place_fixture_with(number: u128, account: &str, symbol: &str, cycle: &str) -> PlaceFixture {
        let request_id = request(number);
        let attribution = attribution(cycle, "ENTRY");
        let command = PlaceOrder {
            request_id,
            created_ts: Utc.with_ymd_and_hms(2026, 8, 10, 9, 0, 0).unwrap(),
            ttl_ms: Some(5_000),
            account_id: BrokerAccountId::new(account),
            client_order_id: ClientOrderId::from_strategy_request(request_id),
            instrument: instrument(symbol),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            qty: Decimal::ONE,
            limit_price: Some(Decimal::new(2210, 1)),
            time_in_force: TimeInForce::Day,
            comment: Some(attribution.internal_comment().to_string()),
        };
        let identity = Stage6DurableRequestIdentityV1::from_place(&command, attribution).unwrap();
        PlaceFixture { command, identity }
    }

    fn cancel_fixture(number: u128, target: &str) -> CancelFixture {
        let command = CancelOrder {
            request_id: request(number),
            created_ts: Utc.with_ymd_and_hms(2026, 8, 10, 9, 1, 0).unwrap(),
            ttl_ms: Some(5_000),
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            order_id: BrokerOrderId::new(target),
            client_order_id: Some(ClientOrderId::from_strategy_request(request(1))),
        };
        let identity = Stage6DurableRequestIdentityV1::from_cancel(
            &command,
            instrument("IMOEXF"),
            attribution("cycle0001", "CANCEL"),
        )
        .unwrap();
        CancelFixture { command, identity }
    }

    fn accepted_place(fixture: &PlaceFixture) -> Stage6JournalRecordV1 {
        accepted_place_with_links(fixture, None, None, digest('1'))
    }

    fn accepted_place_with_links(
        fixture: &PlaceFixture,
        previous: Option<Stage6JournalRecordId>,
        causal: Option<Stage6JournalRecordId>,
        source: Stage6Sha256Digest,
    ) -> Stage6JournalRecordV1 {
        let snapshot =
            Stage6DurableCommandSnapshotV1::from_place(&fixture.identity, &fixture.command)
                .unwrap();
        Stage6JournalRecordV1::request_accepted(
            fixture.identity.clone(),
            snapshot,
            Stage6LifecycleSequence::new(1).unwrap(),
            previous,
            causal,
            source,
        )
        .unwrap()
    }

    fn accepted_cancel(fixture: &CancelFixture) -> Stage6JournalRecordV1 {
        let snapshot =
            Stage6DurableCommandSnapshotV1::from_cancel(&fixture.identity, &fixture.command)
                .unwrap();
        Stage6JournalRecordV1::request_accepted(
            fixture.identity.clone(),
            snapshot,
            Stage6LifecycleSequence::new(1).unwrap(),
            None,
            None,
            digest('1'),
        )
        .unwrap()
    }

    fn dispatch(
        identity: &Stage6DurableRequestIdentityV1,
        accepted: &Stage6JournalRecordV1,
        ordinal: u32,
        sequence: u64,
        previous: &Stage6JournalRecordV1,
    ) -> Stage6JournalRecordV1 {
        Stage6JournalRecordV1::dispatch_attempt_recorded(
            identity.clone(),
            ordinal,
            accepted.canonical_payload_sha256().clone(),
            Stage6LifecycleSequence::new(sequence).unwrap(),
            Some(previous.journal_record_id().clone()),
            digest('2'),
        )
        .unwrap()
    }

    fn reconcile(
        identity: &Stage6DurableRequestIdentityV1,
        disposition: Stage6ReconciliationDispositionV1,
        sequence: u64,
        previous: &Stage6JournalRecordV1,
    ) -> Stage6JournalRecordV1 {
        Stage6JournalRecordV1::reconciliation_observed(
            identity.clone(),
            disposition,
            Stage6LifecycleSequence::new(sequence).unwrap(),
            Some(previous.journal_record_id().clone()),
            digest('3'),
        )
        .unwrap()
    }

    fn broker_order(
        identity: &Stage6DurableRequestIdentityV1,
        order_id: &str,
        sequence: u64,
        previous: &Stage6JournalRecordV1,
    ) -> Stage6JournalRecordV1 {
        Stage6JournalRecordV1::broker_order_observed(
            identity.clone(),
            BrokerOrderId::new(order_id),
            Stage6LifecycleSequence::new(sequence).unwrap(),
            Some(previous.journal_record_id().clone()),
            digest('4'),
        )
        .unwrap()
    }

    fn broker_trade(
        identity: &Stage6DurableRequestIdentityV1,
        trade_id: &str,
        order_id: &str,
        sequence: u64,
        previous: &Stage6JournalRecordV1,
    ) -> Stage6JournalRecordV1 {
        Stage6JournalRecordV1::broker_trade_observed(
            identity.clone(),
            BrokerTradeId::new(trade_id),
            BrokerOrderId::new(order_id),
            Stage6LifecycleSequence::new(sequence).unwrap(),
            Some(previous.journal_record_id().clone()),
            digest('5'),
        )
        .unwrap()
    }

    fn replay(records: &[Stage6JournalRecordV1]) -> Stage6ReplaySnapshotV1 {
        Stage6ReplayEngineV1::replay(records).unwrap()
    }

    fn only(snapshot: &Stage6ReplaySnapshotV1) -> &Stage6RecoveredRequestV1 {
        assert_eq!(snapshot.requests().len(), 1);
        &snapshot.requests()[0]
    }

    #[test]
    fn stage6c_cw1_no_record_means_no_durable_request() {
        assert!(replay(&[]).requests().is_empty());
    }

    #[test]
    fn stage6c_cw2_request_accepted_is_ready_for_first_dispatch() {
        let accepted = accepted_place(&place_fixture(1));
        assert_eq!(
            only(&replay(&[accepted])).dispatch_safety_state(),
            Stage6DispatchSafetyStateV1::ReadyForFirstDispatch
        );
    }

    #[test]
    fn stage6c_cw3_dispatch_attempt_requires_reconciliation() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        assert_eq!(
            only(&replay(&[accepted, attempt])).dispatch_safety_state(),
            Stage6DispatchSafetyStateV1::ReconciliationRequired
        );
    }

    #[test]
    fn stage6c_cw4_lost_broker_response_remains_reconciliation_required() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let snapshot = replay(&[accepted, attempt]);
        assert!(only(&snapshot).known_broker_order_id().is_none());
    }

    #[test]
    fn stage6c_cw5_reconciliation_recovers_broker_order_and_forbids_dispatch() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let found = reconcile(
            &fixture.identity,
            Stage6ReconciliationDispositionV1::BrokerOrderFound {
                broker_order_id: BrokerOrderId::new("ORDER/FOUND"),
            },
            3,
            &attempt,
        );
        let snapshot = replay(&[accepted, attempt, found]);
        assert_eq!(
            only(&snapshot).known_broker_order_id().unwrap().as_str(),
            "ORDER/FOUND"
        );
        assert_eq!(
            only(&snapshot).dispatch_safety_state(),
            Stage6DispatchSafetyStateV1::DispatchForbidden
        );
    }

    #[test]
    fn stage6c_cw6_trade_establishes_order_without_finalizing() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let trade = broker_trade(&fixture.identity, "TRADE/1", "ORDER/1", 3, &attempt);
        let snapshot = replay(&[accepted, attempt, trade]);
        assert_eq!(only(&snapshot).observed_broker_trade_ids().len(), 1);
        assert!(only(&snapshot).final_disposition().is_none());
    }

    #[test]
    fn stage6c_cw7_cancel_without_outcome_stays_reconciliation_required() {
        let fixture = cancel_fixture(2, "ORDER/TARGET");
        let accepted = accepted_cancel(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        assert_eq!(
            only(&replay(&[accepted, attempt])).dispatch_safety_state(),
            Stage6DispatchSafetyStateV1::ReconciliationRequired
        );
    }

    #[test]
    fn stage6c_cw8_restart_replay_fingerprint_is_identical() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let records = vec![accepted, attempt];
        assert_eq!(
            replay(&records).semantic_fingerprint_sha256().as_str(),
            include_str!("../../../fixtures/stage6c/replay-fingerprint-v1.txt").trim()
        );
        assert_eq!(
            replay(&records).semantic_fingerprint_sha256(),
            replay(&records).semantic_fingerprint_sha256()
        );
    }

    #[test]
    fn stage6c_cw9_exact_duplicate_after_restart_is_idempotent() {
        let accepted = accepted_place(&place_fixture(1));
        let original = replay(std::slice::from_ref(&accepted));
        let duplicated = replay(&[accepted.clone(), accepted]);
        assert_eq!(
            original.semantic_fingerprint_sha256(),
            duplicated.semantic_fingerprint_sha256()
        );
    }

    #[test]
    fn stage6c_cw10_conflicting_same_id_fails_closed() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let conflicting = Stage6JournalRecordV1::broker_order_observed(
            fixture.identity,
            BrokerOrderId::new("ORDER/CONFLICT"),
            Stage6LifecycleSequence::new(1).unwrap(),
            None,
            digest('4'),
        )
        .unwrap();
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[accepted, conflicting]).unwrap_err(),
            Stage6ReplayError::ConflictingReplay
        );
    }

    #[test]
    fn stage6c_no_order_reconciliation_enables_same_identity_retry() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let first = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let no_order = reconcile(
            &fixture.identity,
            Stage6ReconciliationDispositionV1::NoBrokerOrderFound,
            3,
            &first,
        );
        let snapshot = replay(&[accepted, first, no_order]);
        assert_eq!(
            only(&snapshot).dispatch_safety_state(),
            Stage6DispatchSafetyStateV1::RetryEligibleSameIdentity
        );
    }

    #[test]
    fn stage6c_next_dispatch_after_no_order_uses_next_ordinal() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let first = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let no_order = reconcile(
            &fixture.identity,
            Stage6ReconciliationDispositionV1::NoBrokerOrderFound,
            3,
            &first,
        );
        let second = dispatch(&fixture.identity, &accepted, 2, 4, &no_order);
        let snapshot = replay(&[accepted, first, no_order, second]);
        assert_eq!(only(&snapshot).dispatch_attempt_count(), 2);
        assert_eq!(
            only(&snapshot).dispatch_safety_state(),
            Stage6DispatchSafetyStateV1::ReconciliationRequired
        );
    }

    #[test]
    fn stage6c_broker_order_observation_forbids_dispatch() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let order = broker_order(&fixture.identity, "ORDER/1", 3, &attempt);
        let snapshot = replay(&[accepted, attempt, order]);
        assert_eq!(
            only(&snapshot).dispatch_safety_state(),
            Stage6DispatchSafetyStateV1::DispatchForbidden
        );
    }

    #[test]
    fn stage6c_repeated_trade_id_same_order_is_idempotent() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let first = broker_trade(&fixture.identity, "TRADE/1", "ORDER/1", 3, &attempt);
        let second = broker_trade(&fixture.identity, "TRADE/1", "ORDER/1", 4, &first);
        let snapshot = replay(&[accepted, attempt, first, second]);
        assert_eq!(only(&snapshot).observed_broker_trade_ids().len(), 1);
        assert_eq!(only(&snapshot).last_unique_sequence(), 4);
    }

    #[test]
    fn stage6c_inconclusive_reconciliation_stays_required() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let inconclusive = reconcile(
            &fixture.identity,
            Stage6ReconciliationDispositionV1::Inconclusive,
            3,
            &attempt,
        );
        assert_eq!(
            only(&replay(&[accepted, attempt, inconclusive])).dispatch_safety_state(),
            Stage6DispatchSafetyStateV1::ReconciliationRequired
        );
    }

    fn cancel_outcome_snapshot(outcome: Stage6CancelOutcomeV1) -> Stage6ReplaySnapshotV1 {
        let fixture = cancel_fixture(2, "ORDER/TARGET");
        let accepted = accepted_cancel(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let outcome_record = Stage6JournalRecordV1::cancel_outcome_observed(
            fixture.identity,
            BrokerOrderId::new("ORDER/TARGET"),
            outcome,
            Stage6LifecycleSequence::new(3).unwrap(),
            Some(attempt.journal_record_id().clone()),
            digest('4'),
        )
        .unwrap();
        replay(&[accepted, attempt, outcome_record])
    }

    #[test]
    fn stage6c_cancel_canceled_outcome_is_preserved() {
        assert_eq!(
            only(&cancel_outcome_snapshot(Stage6CancelOutcomeV1::Canceled)).cancel_outcome(),
            Some(Stage6CancelOutcomeV1::Canceled)
        );
    }

    #[test]
    fn stage6c_cancel_already_terminal_outcome_is_preserved() {
        assert_eq!(
            only(&cancel_outcome_snapshot(
                Stage6CancelOutcomeV1::AlreadyTerminalNonExecution
            ))
            .cancel_outcome(),
            Some(Stage6CancelOutcomeV1::AlreadyTerminalNonExecution)
        );
    }

    #[test]
    fn stage6c_cancel_execution_outcome_is_not_normalized_to_canceled() {
        assert_eq!(
            only(&cancel_outcome_snapshot(
                Stage6CancelOutcomeV1::ExecutionObserved
            ))
            .cancel_outcome(),
            Some(Stage6CancelOutcomeV1::ExecutionObserved)
        );
    }

    #[test]
    fn stage6c_request_finalization_is_explicit_and_terminal() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let finalized = Stage6JournalRecordV1::request_finalized(
            fixture.identity,
            Stage6RequestFinalDispositionV1::Completed,
            Stage6LifecycleSequence::new(2).unwrap(),
            Some(accepted.journal_record_id().clone()),
            digest('6'),
        )
        .unwrap();
        let snapshot = replay(&[accepted, finalized]);
        assert_eq!(
            only(&snapshot).final_disposition(),
            Some(Stage6RequestFinalDispositionV1::Completed)
        );
    }

    #[test]
    fn stage6c_exact_duplicate_before_later_record_is_idempotent() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let snapshot = replay(&[accepted.clone(), accepted, attempt]);
        assert_eq!(only(&snapshot).last_unique_sequence(), 2);
    }

    #[test]
    fn stage6c_exact_duplicate_after_later_record_is_idempotent() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let snapshot = replay(&[accepted.clone(), attempt, accepted]);
        assert_eq!(only(&snapshot).last_unique_sequence(), 2);
    }

    #[test]
    fn stage6c_interleaved_request_permutations_have_same_semantic_fingerprint() {
        let first_fixture = place_fixture(1);
        let second_fixture = place_fixture(2);
        let first_accepted = accepted_place(&first_fixture);
        let second_accepted = accepted_place(&second_fixture);
        let first_attempt = dispatch(
            &first_fixture.identity,
            &first_accepted,
            1,
            2,
            &first_accepted,
        );
        let second_attempt = dispatch(
            &second_fixture.identity,
            &second_accepted,
            1,
            2,
            &second_accepted,
        );
        let left = replay(&[
            first_accepted.clone(),
            second_accepted.clone(),
            first_attempt.clone(),
            second_attempt.clone(),
        ]);
        let right = replay(&[
            second_accepted,
            first_accepted,
            second_attempt,
            first_attempt,
        ]);
        assert_eq!(
            left.semantic_fingerprint_sha256(),
            right.semantic_fingerprint_sha256()
        );
    }

    #[test]
    fn stage6c_memory_backend_records_replay_identically() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let direct = replay(&[accepted.clone(), attempt.clone()]);
        let mut backend = Stage6MemoryJournalBackend::new();
        backend.append(&accepted).unwrap();
        backend.append(&attempt).unwrap();
        let stored = replay(backend.records());
        assert_eq!(
            direct.semantic_fingerprint_sha256(),
            stored.semantic_fingerprint_sha256()
        );
    }

    #[test]
    fn stage6c_old_stage6a_place_golden_is_byte_identical_and_decodable() {
        let bytes = include_bytes!("../../../fixtures/stage6a/place-request-accepted-v1.json")
            .strip_suffix(b"\n")
            .unwrap();
        assert_eq!(
            Stage6JournalRecordV1::decode_canonical(bytes)
                .unwrap()
                .encode_canonical(),
            bytes
        );
    }

    #[test]
    fn stage6c_old_stage6a_cancel_golden_is_byte_identical_and_decodable() {
        let bytes = include_bytes!("../../../fixtures/stage6a/cancel-request-accepted-v1.json")
            .strip_suffix(b"\n")
            .unwrap();
        assert_eq!(
            Stage6JournalRecordV1::decode_canonical(bytes)
                .unwrap()
                .encode_canonical(),
            bytes
        );
    }

    #[test]
    fn stage6c_old_stage6b_one_frame_golden_is_scannable_and_replayable() {
        let bytes =
            decode_hex(include_str!("../../../fixtures/stage6b/place-one-frame-v1.hex").trim());
        let backend = Stage6MemoryJournalBackend::from_framed_bytes(bytes).unwrap();
        assert_eq!(replay(backend.records()).requests().len(), 1);
    }

    #[test]
    fn stage6c_first_record_must_be_request_accepted() {
        let fixture = place_fixture(1);
        let order = Stage6JournalRecordV1::broker_order_observed(
            fixture.identity,
            BrokerOrderId::new("ORDER/1"),
            Stage6LifecycleSequence::new(1).unwrap(),
            None,
            digest('4'),
        )
        .unwrap();
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[order]).unwrap_err(),
            Stage6ReplayError::SequenceStartInvalid
        );
    }

    #[test]
    fn stage6c_first_sequence_must_be_one() {
        let fixture = place_fixture(1);
        let snapshot =
            Stage6DurableCommandSnapshotV1::from_place(&fixture.identity, &fixture.command)
                .unwrap();
        let record = Stage6JournalRecordV1::request_accepted(
            fixture.identity,
            snapshot,
            Stage6LifecycleSequence::new(2).unwrap(),
            None,
            None,
            digest('1'),
        )
        .unwrap();
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[record]).unwrap_err(),
            Stage6ReplayError::SequenceStartInvalid
        );
    }

    #[test]
    fn stage6c_first_previous_record_must_be_none() {
        let other = accepted_place(&place_fixture(2));
        let fixture = place_fixture(1);
        let record = accepted_place_with_links(
            &fixture,
            Some(other.journal_record_id().clone()),
            None,
            digest('1'),
        );
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[record]).unwrap_err(),
            Stage6ReplayError::SequenceStartInvalid
        );
    }

    #[test]
    fn stage6c_sequence_gap_fails_closed() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let gap = dispatch(&fixture.identity, &accepted, 1, 3, &accepted);
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[accepted, gap]).unwrap_err(),
            Stage6ReplayError::SequenceGap
        );
    }

    #[test]
    fn stage6c_sequence_regression_fails_closed_as_conflicting_replay() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let regression = Stage6JournalRecordV1::broker_order_observed(
            fixture.identity,
            BrokerOrderId::new("ORDER/1"),
            Stage6LifecycleSequence::new(1).unwrap(),
            None,
            digest('4'),
        )
        .unwrap();
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[accepted, regression]).unwrap_err(),
            Stage6ReplayError::ConflictingReplay
        );
    }

    #[test]
    fn stage6c_wrong_previous_record_fails_closed() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let other = accepted_place(&place_fixture(2));
        let wrong = Stage6JournalRecordV1::dispatch_attempt_recorded(
            fixture.identity,
            1,
            accepted.canonical_payload_sha256().clone(),
            Stage6LifecycleSequence::new(2).unwrap(),
            Some(other.journal_record_id().clone()),
            digest('2'),
        )
        .unwrap();
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[other, accepted, wrong]).unwrap_err(),
            Stage6ReplayError::PreviousRecordMismatch
        );
    }

    fn assert_identity_drift(changed: PlaceFixture) {
        let original = place_fixture(1);
        let accepted = accepted_place(&original);
        let attempt = Stage6JournalRecordV1::dispatch_attempt_recorded(
            changed.identity,
            1,
            accepted.canonical_payload_sha256().clone(),
            Stage6LifecycleSequence::new(2).unwrap(),
            Some(accepted.journal_record_id().clone()),
            digest('2'),
        )
        .unwrap();
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[accepted, attempt]).unwrap_err(),
            Stage6ReplayError::IdentityDrift
        );
    }

    #[test]
    fn stage6c_account_drift_fails_closed() {
        assert_identity_drift(place_fixture_with(
            1,
            "ACC_TEST_0002",
            "IMOEXF",
            "cycle0001",
        ));
    }

    #[test]
    fn stage6c_instrument_drift_fails_closed() {
        assert_identity_drift(place_fixture_with(
            1,
            "ACC_TEST_0001",
            "RTS-9.26",
            "cycle0001",
        ));
    }

    #[test]
    fn stage6c_attribution_drift_fails_closed() {
        assert_identity_drift(place_fixture_with(
            1,
            "ACC_TEST_0001",
            "IMOEXF",
            "cycle0002",
        ));
    }

    #[test]
    fn stage6c_action_drift_fails_closed() {
        let original = place_fixture(2);
        let accepted = accepted_place(&original);
        let changed = cancel_fixture(2, "ORDER/TARGET");
        let attempt = Stage6JournalRecordV1::dispatch_attempt_recorded(
            changed.identity,
            1,
            accepted.canonical_payload_sha256().clone(),
            Stage6LifecycleSequence::new(2).unwrap(),
            Some(accepted.journal_record_id().clone()),
            digest('2'),
        )
        .unwrap();
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[accepted, attempt]).unwrap_err(),
            Stage6ReplayError::IdentityDrift
        );
    }

    #[test]
    fn stage6c_cancel_target_drift_fails_closed() {
        let original = cancel_fixture(2, "ORDER/A");
        let accepted = accepted_cancel(&original);
        let changed = cancel_fixture(2, "ORDER/B");
        let attempt = Stage6JournalRecordV1::dispatch_attempt_recorded(
            changed.identity,
            1,
            accepted.canonical_payload_sha256().clone(),
            Stage6LifecycleSequence::new(2).unwrap(),
            Some(accepted.journal_record_id().clone()),
            digest('2'),
        )
        .unwrap();
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[accepted, attempt]).unwrap_err(),
            Stage6ReplayError::CancelTargetConflict
        );
    }

    #[test]
    fn stage6c_same_record_id_changed_source_evidence_is_conflict() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let first = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let changed = Stage6JournalRecordV1::dispatch_attempt_recorded(
            fixture.identity,
            1,
            accepted.canonical_payload_sha256().clone(),
            Stage6LifecycleSequence::new(2).unwrap(),
            Some(accepted.journal_record_id().clone()),
            digest('9'),
        )
        .unwrap();
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[accepted, first, changed]).unwrap_err(),
            Stage6ReplayError::ConflictingReplay
        );
    }

    #[test]
    fn stage6c_same_record_id_changed_causal_parent_is_conflict() {
        let parent = accepted_place(&place_fixture(2));
        let fixture = place_fixture(1);
        let original = accepted_place(&fixture);
        let changed = accepted_place_with_links(
            &fixture,
            None,
            Some(parent.journal_record_id().clone()),
            digest('1'),
        );
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[parent, original, changed]).unwrap_err(),
            Stage6ReplayError::ConflictingReplay
        );
    }

    #[test]
    fn stage6c_dispatch_ordinal_zero_is_rejected_by_record_authority() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        assert!(Stage6JournalRecordV1::dispatch_attempt_recorded(
            fixture.identity,
            0,
            accepted.canonical_payload_sha256().clone(),
            Stage6LifecycleSequence::new(2).unwrap(),
            Some(accepted.journal_record_id().clone()),
            digest('2'),
        )
        .is_err());
    }

    #[test]
    fn stage6c_dispatch_ordinal_gap_fails_closed() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 2, 2, &accepted);
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[accepted, attempt]).unwrap_err(),
            Stage6ReplayError::DispatchAttemptInvalid
        );
    }

    #[test]
    fn stage6c_second_dispatch_while_unresolved_is_blind_redispatch() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let first = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let second = dispatch(&fixture.identity, &accepted, 2, 3, &first);
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[accepted, first, second]).unwrap_err(),
            Stage6ReplayError::BlindRedispatchBlocked
        );
    }

    #[test]
    fn stage6c_changed_request_cannot_claim_retry_ordinal_two() {
        let fixture = place_fixture(2);
        let accepted = accepted_place(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 2, 2, &accepted);
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[accepted, attempt]).unwrap_err(),
            Stage6ReplayError::DispatchAttemptInvalid
        );
    }

    #[test]
    fn stage6c_different_broker_order_for_same_request_is_conflict() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let first = broker_order(&fixture.identity, "ORDER/1", 3, &attempt);
        let second = broker_order(&fixture.identity, "ORDER/2", 4, &first);
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[accepted, attempt, first, second]).unwrap_err(),
            Stage6ReplayError::BrokerOrderConflict
        );
    }

    #[test]
    fn stage6c_same_trade_id_with_different_order_is_conflict() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let first = broker_trade(&fixture.identity, "TRADE/1", "ORDER/1", 3, &attempt);
        let second = broker_trade(&fixture.identity, "TRADE/1", "ORDER/2", 4, &first);
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[accepted, attempt, first, second]).unwrap_err(),
            Stage6ReplayError::BrokerTradeConflict
        );
    }

    #[test]
    fn stage6c_trade_against_known_different_order_is_conflict() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let order = broker_order(&fixture.identity, "ORDER/1", 3, &attempt);
        let trade = broker_trade(&fixture.identity, "TRADE/1", "ORDER/2", 4, &order);
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[accepted, attempt, order, trade]).unwrap_err(),
            Stage6ReplayError::BrokerOrderConflict
        );
    }

    #[test]
    fn stage6c_cancel_outcome_target_mismatch_is_rejected_by_record_authority() {
        let fixture = cancel_fixture(2, "ORDER/A");
        let accepted = accepted_cancel(&fixture);
        assert!(Stage6JournalRecordV1::cancel_outcome_observed(
            fixture.identity,
            BrokerOrderId::new("ORDER/B"),
            Stage6CancelOutcomeV1::Canceled,
            Stage6LifecycleSequence::new(2).unwrap(),
            Some(accepted.journal_record_id().clone()),
            digest('4'),
        )
        .is_err());
    }

    #[test]
    fn stage6c_cancel_outcome_on_place_is_rejected_by_record_authority() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        assert!(Stage6JournalRecordV1::cancel_outcome_observed(
            fixture.identity,
            BrokerOrderId::new("ORDER/A"),
            Stage6CancelOutcomeV1::Canceled,
            Stage6LifecycleSequence::new(2).unwrap(),
            Some(accepted.journal_record_id().clone()),
            digest('4'),
        )
        .is_err());
    }

    #[test]
    fn stage6c_reconciliation_with_conflicting_broker_order_fails_closed() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let order = broker_order(&fixture.identity, "ORDER/1", 3, &attempt);
        let found = reconcile(
            &fixture.identity,
            Stage6ReconciliationDispositionV1::BrokerOrderFound {
                broker_order_id: BrokerOrderId::new("ORDER/2"),
            },
            4,
            &order,
        );
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[accepted, attempt, order, found]).unwrap_err(),
            Stage6ReplayError::BrokerOrderConflict
        );
    }

    #[test]
    fn stage6c_event_after_finalization_fails_closed() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let finalized = Stage6JournalRecordV1::request_finalized(
            fixture.identity.clone(),
            Stage6RequestFinalDispositionV1::Completed,
            Stage6LifecycleSequence::new(2).unwrap(),
            Some(accepted.journal_record_id().clone()),
            digest('6'),
        )
        .unwrap();
        let later = broker_order(&fixture.identity, "ORDER/1", 3, &finalized);
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[accepted, finalized, later]).unwrap_err(),
            Stage6ReplayError::EventAfterFinalization
        );
    }

    #[test]
    fn stage6c_different_second_finalization_fails_closed() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let first = Stage6JournalRecordV1::request_finalized(
            fixture.identity.clone(),
            Stage6RequestFinalDispositionV1::Completed,
            Stage6LifecycleSequence::new(2).unwrap(),
            Some(accepted.journal_record_id().clone()),
            digest('6'),
        )
        .unwrap();
        let second = Stage6JournalRecordV1::request_finalized(
            fixture.identity,
            Stage6RequestFinalDispositionV1::Rejected,
            Stage6LifecycleSequence::new(3).unwrap(),
            Some(first.journal_record_id().clone()),
            digest('7'),
        )
        .unwrap();
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[accepted, first, second]).unwrap_err(),
            Stage6ReplayError::EventAfterFinalization
        );
    }

    #[test]
    fn stage6c_nonexistent_causal_parent_fails_closed() {
        let parent = accepted_place(&place_fixture(2));
        let fixture = place_fixture(1);
        let record = accepted_place_with_links(
            &fixture,
            None,
            Some(parent.journal_record_id().clone()),
            digest('1'),
        );
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[record]).unwrap_err(),
            Stage6ReplayError::CausalParentMissing
        );
    }

    #[test]
    fn stage6c_future_causal_parent_fails_closed() {
        let parent = accepted_place(&place_fixture(2));
        let fixture = place_fixture(1);
        let child = accepted_place_with_links(
            &fixture,
            None,
            Some(parent.journal_record_id().clone()),
            digest('1'),
        );
        assert_eq!(
            Stage6ReplayEngineV1::replay(&[child, parent]).unwrap_err(),
            Stage6ReplayError::CausalParentMissing
        );
    }

    #[test]
    fn stage6c_explicit_conflict_audit_is_preserved_and_forbids_dispatch() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let conflict = Stage6JournalRecordV1::conflict_observed(
            fixture.identity,
            crate::Stage6ConflictKindV1::ConflictingReplay,
            Some(accepted.journal_record_id().clone()),
            Some(digest('1')),
            Some(digest('2')),
            Stage6LifecycleSequence::new(2).unwrap(),
            Some(accepted.journal_record_id().clone()),
            digest('7'),
        )
        .unwrap();
        let snapshot = replay(&[accepted, conflict]);
        assert!(only(&snapshot).conflict_observed());
        assert_eq!(
            only(&snapshot).dispatch_safety_state(),
            Stage6DispatchSafetyStateV1::DispatchForbidden
        );
    }

    fn cancel_records_with_outcomes(
        outcomes: &[Stage6CancelOutcomeV1],
    ) -> Vec<Stage6JournalRecordV1> {
        let fixture = cancel_fixture(2, "ORDER/TARGET");
        let accepted = accepted_cancel(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let mut records = vec![accepted, attempt];
        for (index, outcome) in outcomes.iter().enumerate() {
            let previous = records.last().unwrap().journal_record_id().clone();
            records.push(
                Stage6JournalRecordV1::cancel_outcome_observed(
                    fixture.identity.clone(),
                    BrokerOrderId::new("ORDER/TARGET"),
                    *outcome,
                    Stage6LifecycleSequence::new(index as u64 + 3).unwrap(),
                    Some(previous),
                    digest('8'),
                )
                .unwrap(),
            );
        }
        records
    }

    fn assert_second_cancel_outcome_rejected(
        first: Stage6CancelOutcomeV1,
        second: Stage6CancelOutcomeV1,
    ) {
        assert_eq!(
            Stage6ReplayEngineV1::replay(&cancel_records_with_outcomes(&[first, second]))
                .unwrap_err(),
            Stage6ReplayError::CancelOutcomeConflict
        );
    }

    #[test]
    fn stage6c_r1_cancel_rejected_first_outcome_is_preserved() {
        assert_eq!(
            only(&cancel_outcome_snapshot(Stage6CancelOutcomeV1::Rejected)).cancel_outcome(),
            Some(Stage6CancelOutcomeV1::Rejected)
        );
    }

    #[test]
    fn stage6c_r1_cancel_inconclusive_stays_reconciliation_required() {
        let fixture = cancel_fixture(2, "ORDER/TARGET");
        let accepted = accepted_cancel(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let inconclusive = reconcile(
            &fixture.identity,
            Stage6ReconciliationDispositionV1::Inconclusive,
            3,
            &attempt,
        );
        let snapshot = replay(&[accepted, attempt, inconclusive]);
        assert_eq!(
            only(&snapshot).dispatch_safety_state(),
            Stage6DispatchSafetyStateV1::ReconciliationRequired
        );
        assert_ne!(
            only(&snapshot).dispatch_safety_state(),
            Stage6DispatchSafetyStateV1::RetryEligibleSameIdentity
        );
    }

    #[test]
    fn stage6c_r1_finalization_preserves_execution_observed_cancel_truth() {
        let fixture = cancel_fixture(2, "ORDER/TARGET");
        let accepted = accepted_cancel(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let outcome = Stage6JournalRecordV1::cancel_outcome_observed(
            fixture.identity.clone(),
            BrokerOrderId::new("ORDER/TARGET"),
            Stage6CancelOutcomeV1::ExecutionObserved,
            Stage6LifecycleSequence::new(3).unwrap(),
            Some(attempt.journal_record_id().clone()),
            digest('8'),
        )
        .unwrap();
        let finalized = Stage6JournalRecordV1::request_finalized(
            fixture.identity,
            Stage6RequestFinalDispositionV1::Completed,
            Stage6LifecycleSequence::new(4).unwrap(),
            Some(outcome.journal_record_id().clone()),
            digest('9'),
        )
        .unwrap();
        let snapshot = replay(&[accepted, attempt, outcome, finalized]);
        assert_eq!(
            only(&snapshot).cancel_outcome(),
            Some(Stage6CancelOutcomeV1::ExecutionObserved)
        );
        assert_eq!(
            only(&snapshot).dispatch_safety_state(),
            Stage6DispatchSafetyStateV1::DispatchForbidden
        );
    }

    #[test]
    fn stage6c_r1_execution_then_canceled_fails_closed() {
        assert_second_cancel_outcome_rejected(
            Stage6CancelOutcomeV1::ExecutionObserved,
            Stage6CancelOutcomeV1::Canceled,
        );
    }

    #[test]
    fn stage6c_r1_execution_then_rejected_fails_closed() {
        assert_second_cancel_outcome_rejected(
            Stage6CancelOutcomeV1::ExecutionObserved,
            Stage6CancelOutcomeV1::Rejected,
        );
    }

    #[test]
    fn stage6c_r1_canceled_then_execution_fails_closed() {
        assert_second_cancel_outcome_rejected(
            Stage6CancelOutcomeV1::Canceled,
            Stage6CancelOutcomeV1::ExecutionObserved,
        );
    }

    #[test]
    fn stage6c_r1_canceled_then_rejected_fails_closed() {
        assert_second_cancel_outcome_rejected(
            Stage6CancelOutcomeV1::Canceled,
            Stage6CancelOutcomeV1::Rejected,
        );
    }

    #[test]
    fn stage6c_r1_rejected_then_canceled_fails_closed() {
        assert_second_cancel_outcome_rejected(
            Stage6CancelOutcomeV1::Rejected,
            Stage6CancelOutcomeV1::Canceled,
        );
    }

    #[test]
    fn stage6c_r1_already_terminal_then_canceled_fails_closed() {
        assert_second_cancel_outcome_rejected(
            Stage6CancelOutcomeV1::AlreadyTerminalNonExecution,
            Stage6CancelOutcomeV1::Canceled,
        );
    }

    #[test]
    fn stage6c_r1_same_outcome_as_new_sequence_fails_closed() {
        assert_second_cancel_outcome_rejected(
            Stage6CancelOutcomeV1::Canceled,
            Stage6CancelOutcomeV1::Canceled,
        );
    }

    #[test]
    fn stage6c_r1_exact_cancel_outcome_replay_remains_idempotent() {
        let mut records = cancel_records_with_outcomes(&[Stage6CancelOutcomeV1::Canceled]);
        records.push(records.last().unwrap().clone());
        let snapshot = replay(&records);
        assert_eq!(
            only(&snapshot).cancel_outcome(),
            Some(Stage6CancelOutcomeV1::Canceled)
        );
        assert_eq!(only(&snapshot).last_unique_sequence(), 3);
    }

    #[test]
    fn stage6c_r1_cancel_no_broker_order_found_is_rejected_by_record_authority() {
        let fixture = cancel_fixture(2, "ORDER/TARGET");
        let accepted = accepted_cancel(&fixture);
        assert_eq!(
            Stage6JournalRecordV1::reconciliation_observed(
                fixture.identity,
                Stage6ReconciliationDispositionV1::NoBrokerOrderFound,
                Stage6LifecycleSequence::new(2).unwrap(),
                Some(accepted.journal_record_id().clone()),
                digest('3'),
            )
            .unwrap_err(),
            crate::Stage6DurableIdentityError::InvalidActionEvent
        );
    }

    #[test]
    fn stage6c_r1_cancel_broker_order_found_is_rejected_by_record_authority() {
        let fixture = cancel_fixture(2, "ORDER/TARGET");
        let accepted = accepted_cancel(&fixture);
        assert_eq!(
            Stage6JournalRecordV1::reconciliation_observed(
                fixture.identity,
                Stage6ReconciliationDispositionV1::BrokerOrderFound {
                    broker_order_id: BrokerOrderId::new("ORDER/TARGET"),
                },
                Stage6LifecycleSequence::new(2).unwrap(),
                Some(accepted.journal_record_id().clone()),
                digest('3'),
            )
            .unwrap_err(),
            crate::Stage6DurableIdentityError::InvalidActionEvent
        );
    }

    #[test]
    fn stage6c_r1_cancel_broker_order_event_is_rejected_by_record_authority() {
        let fixture = cancel_fixture(2, "ORDER/TARGET");
        let accepted = accepted_cancel(&fixture);
        assert_eq!(
            Stage6JournalRecordV1::broker_order_observed(
                fixture.identity,
                BrokerOrderId::new("ORDER/TARGET"),
                Stage6LifecycleSequence::new(2).unwrap(),
                Some(accepted.journal_record_id().clone()),
                digest('4'),
            )
            .unwrap_err(),
            crate::Stage6DurableIdentityError::InvalidActionEvent
        );
    }

    #[test]
    fn stage6c_r1_cancel_broker_trade_event_is_rejected_by_record_authority() {
        let fixture = cancel_fixture(2, "ORDER/TARGET");
        let accepted = accepted_cancel(&fixture);
        assert_eq!(
            Stage6JournalRecordV1::broker_trade_observed(
                fixture.identity,
                BrokerTradeId::new("TRADE/1"),
                BrokerOrderId::new("ORDER/TARGET"),
                Stage6LifecycleSequence::new(2).unwrap(),
                Some(accepted.journal_record_id().clone()),
                digest('5'),
            )
            .unwrap_err(),
            crate::Stage6DurableIdentityError::InvalidActionEvent
        );
    }

    #[test]
    fn stage6c_r1_cancel_never_enters_retry_eligible_same_identity() {
        let fixture = cancel_fixture(2, "ORDER/TARGET");
        let accepted = accepted_cancel(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        assert_eq!(
            validate_event_for_action(
                Stage6DurableActionKind::Cancel,
                &Stage6JournalPayloadV1::ReconciliationObserved {
                    disposition: Stage6ReconciliationDispositionV1::NoBrokerOrderFound,
                },
            ),
            Err(Stage6ReplayError::InvalidActionEvent)
        );
        let snapshot = replay(&[accepted, attempt]);
        assert_ne!(
            only(&snapshot).dispatch_safety_state(),
            Stage6DispatchSafetyStateV1::RetryEligibleSameIdentity
        );
    }

    #[test]
    fn stage6c_r1_place_no_order_retry_regression_is_preserved() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let no_order = reconcile(
            &fixture.identity,
            Stage6ReconciliationDispositionV1::NoBrokerOrderFound,
            3,
            &attempt,
        );
        assert_eq!(
            only(&replay(&[accepted, attempt, no_order])).dispatch_safety_state(),
            Stage6DispatchSafetyStateV1::RetryEligibleSameIdentity
        );
    }

    #[test]
    fn stage6c_r1_place_broker_order_found_regression_is_preserved() {
        let fixture = place_fixture(1);
        let accepted = accepted_place(&fixture);
        let attempt = dispatch(&fixture.identity, &accepted, 1, 2, &accepted);
        let found = reconcile(
            &fixture.identity,
            Stage6ReconciliationDispositionV1::BrokerOrderFound {
                broker_order_id: BrokerOrderId::new("ORDER/1"),
            },
            3,
            &attempt,
        );
        assert_eq!(
            only(&replay(&[accepted, attempt, found])).dispatch_safety_state(),
            Stage6DispatchSafetyStateV1::DispatchForbidden
        );
    }
}
