use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::de;
use serde::{Deserialize, Deserializer, Serialize};

use crate::command::{CommandAck, CommandAckReason, CommandAckReasonCode, CommandAckStatus};
use crate::ids::{
    deserialize_broker_order_id_legacy_numeric_or_string,
    deserialize_option_broker_order_id_legacy_numeric_or_string,
    deserialize_vec_broker_order_id_legacy_numeric_or_string, BrokerOrderId, BrokerTradeId,
    ClientOrderId, StrategyRequestId, BROKER_ORDER_ID_ENCODING,
    LEGACY_ALOR_NUMERIC_ORDER_ID_IMPORT, RUNTIME_STATE_SCHEMA_VERSION_V2,
};

fn runtime_state_schema_version_v2() -> u16 {
    RUNTIME_STATE_SCHEMA_VERSION_V2
}

fn broker_order_id_encoding_marker() -> String {
    BROKER_ORDER_ID_ENCODING.to_string()
}

fn legacy_alor_numeric_import_marker() -> String {
    LEGACY_ALOR_NUMERIC_ORDER_ID_IMPORT.to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeOrderEvent {
    #[serde(deserialize_with = "deserialize_broker_order_id_legacy_numeric_or_string")]
    pub order_id: BrokerOrderId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<ClientOrderId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exchange: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ts: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeTradeEvent {
    pub trade_id: BrokerTradeId,
    #[serde(deserialize_with = "deserialize_broker_order_id_legacy_numeric_or_string")]
    pub order_id: BrokerOrderId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<ClientOrderId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exchange: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ts: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeBootstrapSnapshotDto {
    #[serde(default)]
    pub working_orders: HashMap<BrokerOrderId, RuntimeOrderEvent>,
    #[serde(default, alias = "working_orders_strategy")]
    pub working_orders_strategy: HashMap<BrokerOrderId, RuntimeOrderEvent>,
    #[serde(
        default,
        deserialize_with = "deserialize_vec_broker_order_id_legacy_numeric_or_string"
    )]
    pub known_order_ids: Vec<BrokerOrderId>,
    #[serde(default)]
    pub account_wide_orders_count: usize,
}

impl RuntimeBootstrapSnapshotDto {
    pub fn validate_for_bootstrap(
        self,
    ) -> Result<ValidatedRuntimeBootstrapSnapshotDto, RuntimeStateValidationError> {
        validate_order_event_map("working_orders", &self.working_orders)?;
        validate_order_event_map("working_orders_strategy", &self.working_orders_strategy)?;
        validate_unique_ids("known_order_ids", &self.known_order_ids)?;

        Ok(ValidatedRuntimeBootstrapSnapshotDto { snapshot: self })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatedRuntimeBootstrapSnapshotDto {
    pub snapshot: RuntimeBootstrapSnapshotDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeStateReadinessBlocker {
    pub kind: RuntimeStateReadinessBlockerKind,
    pub broker_order_id_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeStateReadinessBlockerKind {
    KnownOrderIdMissingFromOrders,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatedRuntimeStateSnapshot {
    pub snapshot: RuntimeStateSnapshot,
    pub readiness_blockers: Vec<RuntimeStateReadinessBlocker>,
}

impl ValidatedRuntimeStateSnapshot {
    pub fn manual_intervention_required(&self) -> bool {
        self.snapshot.manual_intervention_required || !self.readiness_blockers.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeStateValidationError {
    #[error("{map_name} map key does not match payload order_id")]
    OrderMapKeyMismatch {
        map_name: &'static str,
        key_len: usize,
        payload_order_id_len: usize,
    },
    #[error("{field} contains duplicate broker order id")]
    DuplicateBrokerOrderId { field: &'static str, id_len: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeStateSnapshot {
    #[serde(default = "runtime_state_schema_version_v2")]
    pub schema_version: u16,
    #[serde(default = "broker_order_id_encoding_marker")]
    pub broker_order_id_encoding: String,
    #[serde(default = "legacy_alor_numeric_import_marker")]
    pub legacy_alor_numeric_order_id_import: String,
    #[serde(default)]
    pub orders: HashMap<BrokerOrderId, RuntimeOrderEvent>,
    #[serde(
        default,
        deserialize_with = "deserialize_vec_broker_order_id_legacy_numeric_or_string"
    )]
    pub known_order_ids: Vec<BrokerOrderId>,
    #[serde(default)]
    pub trades: Vec<RuntimeTradeEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_entry_request_id: Option<StrategyRequestId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_exit_request_id: Option<StrategyRequestId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deferred_entry_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deferred_exit_state: Option<String>,
    #[serde(default)]
    pub manual_intervention_required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual_intervention_reason: Option<String>,
}

impl RuntimeStateSnapshot {
    pub fn new_v2() -> Self {
        Self {
            schema_version: RUNTIME_STATE_SCHEMA_VERSION_V2,
            broker_order_id_encoding: BROKER_ORDER_ID_ENCODING.to_string(),
            legacy_alor_numeric_order_id_import: LEGACY_ALOR_NUMERIC_ORDER_ID_IMPORT.to_string(),
            orders: HashMap::new(),
            known_order_ids: Vec::new(),
            trades: Vec::new(),
            pending_entry_request_id: None,
            pending_exit_request_id: None,
            deferred_entry_state: None,
            deferred_exit_state: None,
            manual_intervention_required: false,
            manual_intervention_reason: None,
        }
    }

    pub fn migrated_to_v2(mut self) -> Self {
        self.schema_version = RUNTIME_STATE_SCHEMA_VERSION_V2;
        self.broker_order_id_encoding = BROKER_ORDER_ID_ENCODING.to_string();
        self.legacy_alor_numeric_order_id_import = LEGACY_ALOR_NUMERIC_ORDER_ID_IMPORT.to_string();
        self
    }

    pub fn validate_for_runtime_restore(
        self,
    ) -> Result<ValidatedRuntimeStateSnapshot, RuntimeStateValidationError> {
        validate_order_event_map("orders", &self.orders)?;
        validate_unique_ids("known_order_ids", &self.known_order_ids)?;

        let readiness_blockers = self
            .known_order_ids
            .iter()
            .filter(|order_id| !self.orders.contains_key(*order_id))
            .map(|order_id| RuntimeStateReadinessBlocker {
                kind: RuntimeStateReadinessBlockerKind::KnownOrderIdMissingFromOrders,
                broker_order_id_len: order_id.as_str().len(),
            })
            .collect();

        Ok(ValidatedRuntimeStateSnapshot {
            snapshot: self.migrated_to_v2(),
            readiness_blockers,
        })
    }
}

fn validate_order_event_map(
    map_name: &'static str,
    orders: &HashMap<BrokerOrderId, RuntimeOrderEvent>,
) -> Result<(), RuntimeStateValidationError> {
    for (key, payload) in orders {
        if key != &payload.order_id {
            return Err(RuntimeStateValidationError::OrderMapKeyMismatch {
                map_name,
                key_len: key.as_str().len(),
                payload_order_id_len: payload.order_id.as_str().len(),
            });
        }
    }
    Ok(())
}

fn validate_unique_ids(
    field: &'static str,
    ids: &[BrokerOrderId],
) -> Result<(), RuntimeStateValidationError> {
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(RuntimeStateValidationError::DuplicateBrokerOrderId {
                field,
                id_len: id.as_str().len(),
            });
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeCommandAckDto {
    pub request_id: StrategyRequestId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<ClientOrderId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub broker_order_id: Option<BrokerOrderId>,
    pub status: CommandAckStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<CommandAckReason>,
    pub received_ts: DateTime<Utc>,
}

#[derive(Deserialize)]
struct RuntimeCommandAckDtoRaw {
    request_id: StrategyRequestId,
    #[serde(default)]
    client_order_id: Option<ClientOrderId>,
    #[serde(
        default,
        deserialize_with = "deserialize_option_broker_order_id_legacy_numeric_or_string"
    )]
    broker_order_id: Option<BrokerOrderId>,
    #[serde(
        default,
        deserialize_with = "deserialize_option_broker_order_id_legacy_numeric_or_string"
    )]
    broker_order_id_str: Option<BrokerOrderId>,
    status: CommandAckStatus,
    #[serde(default)]
    reason: Option<CommandAckReason>,
    received_ts: DateTime<Utc>,
}

impl<'de> Deserialize<'de> for RuntimeCommandAckDto {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RuntimeCommandAckDtoRaw::deserialize(deserializer)?;
        let broker_order_id = match (raw.broker_order_id, raw.broker_order_id_str) {
            (Some(primary), Some(legacy_string)) if primary != legacy_string => {
                return Err(de::Error::custom(
                    "conflicting broker_order_id and broker_order_id_str",
                ));
            }
            (Some(primary), _) => Some(primary),
            (None, Some(legacy_string)) => Some(legacy_string),
            (None, None) => None,
        };

        Ok(Self {
            request_id: raw.request_id,
            client_order_id: raw.client_order_id,
            broker_order_id,
            status: raw.status,
            reason: raw.reason,
            received_ts: raw.received_ts,
        })
    }
}

impl From<RuntimeCommandAckDto> for CommandAck {
    fn from(value: RuntimeCommandAckDto) -> Self {
        Self {
            request_id: value.request_id,
            client_order_id: value.client_order_id,
            broker_order_id: value.broker_order_id,
            status: value.status,
            reason: value.reason,
            received_ts: value.received_ts,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimePendingRequestIdentity {
    pub request_id: StrategyRequestId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<ClientOrderId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub broker_order_id: Option<BrokerOrderId>,
}

impl RuntimePendingRequestIdentity {
    pub fn evaluate_ack(&self, ack: &CommandAck) -> RuntimeAckLifecycleDecision {
        let request_id_matches = self.request_id == ack.request_id;
        let client_order_id_matches = match (&self.client_order_id, &ack.client_order_id) {
            (Some(left), Some(right)) => left == right,
            _ => false,
        };
        let broker_order_id_matches = match (&self.broker_order_id, &ack.broker_order_id) {
            (Some(left), Some(right)) => left == right,
            _ => false,
        };
        let broker_order_id_state = ack_broker_order_id_state(ack);
        let status_policy = ack_status_policy(ack, broker_order_id_state);
        let mut issues = Vec::new();

        if !request_id_matches {
            issues.push(RuntimeAckLifecycleIssue::RequestIdMismatch);
            if client_order_id_matches {
                issues.push(RuntimeAckLifecycleIssue::ClientOrderIdOnlyMatchDoesNotClearPending);
            }
            if broker_order_id_matches {
                issues.push(RuntimeAckLifecycleIssue::BrokerOrderIdOnlyMatchDoesNotClearPending);
            }

            return RuntimeAckLifecycleDecision {
                request_id_matches,
                client_order_id_matches,
                broker_order_id_matches,
                broker_order_id_state,
                status_policy,
                pending_disposition: RuntimeAckPendingDisposition::KeepPending,
                issues,
            };
        }

        if self.broker_order_id.is_some()
            && ack.broker_order_id.is_some()
            && !broker_order_id_matches
        {
            issues.push(RuntimeAckLifecycleIssue::BrokerOrderIdMismatchForMatchingRequest);
        }

        issues.extend(status_policy.issues());
        let pending_disposition = if !issues.is_empty() {
            RuntimeAckPendingDisposition::KeepPending
        } else {
            status_policy.pending_disposition()
        };

        RuntimeAckLifecycleDecision {
            request_id_matches,
            client_order_id_matches,
            broker_order_id_matches,
            broker_order_id_state,
            status_policy,
            pending_disposition,
            issues,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAckPendingDisposition {
    ClearPending,
    KeepPending,
    KeepPendingBrokerOrderId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAckBrokerOrderIdState {
    Present,
    PendingBrokerOrderId,
    NotRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAckStatusPolicy {
    ClearPending,
    KeepPending,
    KeepPendingBrokerOrderId,
    RequiresPriorOutcome,
    RequiresNoSendProof,
    ManualInterventionRequired,
}

impl RuntimeAckStatusPolicy {
    fn pending_disposition(self) -> RuntimeAckPendingDisposition {
        match self {
            RuntimeAckStatusPolicy::ClearPending => RuntimeAckPendingDisposition::ClearPending,
            RuntimeAckStatusPolicy::KeepPending
            | RuntimeAckStatusPolicy::RequiresPriorOutcome
            | RuntimeAckStatusPolicy::RequiresNoSendProof
            | RuntimeAckStatusPolicy::ManualInterventionRequired => {
                RuntimeAckPendingDisposition::KeepPending
            }
            RuntimeAckStatusPolicy::KeepPendingBrokerOrderId => {
                RuntimeAckPendingDisposition::KeepPendingBrokerOrderId
            }
        }
    }

    fn issues(self) -> impl Iterator<Item = RuntimeAckLifecycleIssue> {
        match self {
            RuntimeAckStatusPolicy::RequiresPriorOutcome => {
                Some(RuntimeAckLifecycleIssue::DuplicateAckRequiresPriorOutcome)
            }
            RuntimeAckStatusPolicy::RequiresNoSendProof => {
                Some(RuntimeAckLifecycleIssue::ExpiredAckRequiresNoSendProof)
            }
            RuntimeAckStatusPolicy::ManualInterventionRequired => {
                Some(RuntimeAckLifecycleIssue::AmbiguousErrorAckDoesNotClearPending)
            }
            RuntimeAckStatusPolicy::ClearPending
            | RuntimeAckStatusPolicy::KeepPending
            | RuntimeAckStatusPolicy::KeepPendingBrokerOrderId => None,
        }
        .into_iter()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAckLifecycleIssue {
    RequestIdMismatch,
    ClientOrderIdOnlyMatchDoesNotClearPending,
    BrokerOrderIdOnlyMatchDoesNotClearPending,
    BrokerOrderIdMismatchForMatchingRequest,
    AmbiguousErrorAckDoesNotClearPending,
    DuplicateAckRequiresPriorOutcome,
    ExpiredAckRequiresNoSendProof,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeAckLifecycleDecision {
    pub request_id_matches: bool,
    pub client_order_id_matches: bool,
    pub broker_order_id_matches: bool,
    pub broker_order_id_state: RuntimeAckBrokerOrderIdState,
    pub status_policy: RuntimeAckStatusPolicy,
    pub pending_disposition: RuntimeAckPendingDisposition,
    pub issues: Vec<RuntimeAckLifecycleIssue>,
}

fn ack_broker_order_id_state(ack: &CommandAck) -> RuntimeAckBrokerOrderIdState {
    if ack.broker_order_id.is_some() {
        RuntimeAckBrokerOrderIdState::Present
    } else if ack_status_requires_or_may_later_receive_broker_order_id(ack.status) {
        RuntimeAckBrokerOrderIdState::PendingBrokerOrderId
    } else {
        RuntimeAckBrokerOrderIdState::NotRequired
    }
}

fn ack_status_requires_or_may_later_receive_broker_order_id(status: CommandAckStatus) -> bool {
    matches!(
        status,
        CommandAckStatus::Accepted | CommandAckStatus::Submitted | CommandAckStatus::Recovered
    )
}

fn ack_status_policy(
    ack: &CommandAck,
    broker_order_id_state: RuntimeAckBrokerOrderIdState,
) -> RuntimeAckStatusPolicy {
    match ack.status {
        CommandAckStatus::Accepted | CommandAckStatus::Submitted | CommandAckStatus::Recovered => {
            if broker_order_id_state == RuntimeAckBrokerOrderIdState::PendingBrokerOrderId {
                RuntimeAckStatusPolicy::KeepPendingBrokerOrderId
            } else {
                RuntimeAckStatusPolicy::ClearPending
            }
        }
        CommandAckStatus::Rejected => RuntimeAckStatusPolicy::ClearPending,
        CommandAckStatus::Timeout | CommandAckStatus::UnknownPending => {
            RuntimeAckStatusPolicy::KeepPending
        }
        CommandAckStatus::Duplicate => RuntimeAckStatusPolicy::RequiresPriorOutcome,
        CommandAckStatus::Expired => match ack.reason.as_ref().map(|reason| reason.code) {
            Some(CommandAckReasonCode::ExpiredCommand) => RuntimeAckStatusPolicy::ClearPending,
            _ => RuntimeAckStatusPolicy::RequiresNoSendProof,
        },
        CommandAckStatus::Error => RuntimeAckStatusPolicy::ManualInterventionRequired,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeOrderEventLifecycle {
    Active,
    Terminal,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeOrderEventLifecycleClassification {
    pub broker_order_id_len: usize,
    pub lifecycle: RuntimeOrderEventLifecycle,
}

impl RuntimeOrderEvent {
    pub fn lifecycle_classification(&self) -> RuntimeOrderEventLifecycleClassification {
        RuntimeOrderEventLifecycleClassification {
            broker_order_id_len: self.order_id.as_str().len(),
            lifecycle: classify_runtime_order_status(self.status.as_deref()),
        }
    }
}

fn classify_runtime_order_status(status: Option<&str>) -> RuntimeOrderEventLifecycle {
    let Some(status) = status else {
        return RuntimeOrderEventLifecycle::Unknown;
    };
    match status.trim().to_ascii_lowercase().as_str() {
        "new" | "accepted" | "submitted" | "working" | "active" | "partially_filled"
        | "partially-filled" | "partial" => RuntimeOrderEventLifecycle::Active,
        "filled" | "done" | "cancelled" | "canceled" | "rejected" | "expired" | "terminal" => {
            RuntimeOrderEventLifecycle::Terminal
        }
        _ => RuntimeOrderEventLifecycle::Unknown,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeBrokerEventReplayDisposition {
    New,
    DuplicateIdempotent,
}

#[derive(Debug, Default, Clone)]
pub struct RuntimeBrokerEventDeduplicator {
    seen_order_events: HashSet<(BrokerOrderId, Option<String>)>,
    seen_trade_events: HashSet<(BrokerTradeId, BrokerOrderId)>,
}

impl RuntimeBrokerEventDeduplicator {
    pub fn classify_order_event(
        &mut self,
        event: &RuntimeOrderEvent,
    ) -> RuntimeBrokerEventReplayDisposition {
        let key = (
            event.order_id.clone(),
            event
                .status
                .as_ref()
                .map(|value| value.trim().to_ascii_lowercase()),
        );
        if self.seen_order_events.insert(key) {
            RuntimeBrokerEventReplayDisposition::New
        } else {
            RuntimeBrokerEventReplayDisposition::DuplicateIdempotent
        }
    }

    pub fn classify_trade_event(
        &mut self,
        event: &RuntimeTradeEvent,
    ) -> RuntimeBrokerEventReplayDisposition {
        let key = (event.trade_id.clone(), event.order_id.clone());
        if self.seen_trade_events.insert(key) {
            RuntimeBrokerEventReplayDisposition::New
        } else {
            RuntimeBrokerEventReplayDisposition::DuplicateIdempotent
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RuntimeCaches {
    #[serde(default)]
    pub orders: HashMap<BrokerOrderId, RuntimeOrderEvent>,
    #[serde(default)]
    pub owned_order_ids: HashSet<BrokerOrderId>,
    #[serde(default)]
    pub observed_order_ids: HashSet<BrokerOrderId>,
    #[serde(default)]
    pub trades_by_order_id: HashMap<BrokerOrderId, Vec<RuntimeTradeEvent>>,
    #[serde(default)]
    pub pending_trades_by_order_id: HashMap<BrokerOrderId, Vec<RuntimeTradeEvent>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_entry: Option<RuntimePendingRequestIdentity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_exit: Option<RuntimePendingRequestIdentity>,
}

impl RuntimeCaches {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_validated_state(validated: ValidatedRuntimeStateSnapshot) -> Self {
        let snapshot = validated.snapshot;
        Self {
            owned_order_ids: snapshot.known_order_ids.into_iter().collect(),
            observed_order_ids: snapshot.orders.keys().cloned().collect(),
            orders: snapshot.orders,
            trades_by_order_id: group_trades_by_order_id(snapshot.trades),
            pending_trades_by_order_id: HashMap::new(),
            pending_entry: snapshot.pending_entry_request_id.map(|request_id| {
                RuntimePendingRequestIdentity {
                    request_id,
                    client_order_id: None,
                    broker_order_id: None,
                }
            }),
            pending_exit: snapshot.pending_exit_request_id.map(|request_id| {
                RuntimePendingRequestIdentity {
                    request_id,
                    client_order_id: None,
                    broker_order_id: None,
                }
            }),
        }
    }

    pub fn order(&self, order_id: &BrokerOrderId) -> Option<&RuntimeOrderEvent> {
        self.orders.get(order_id)
    }

    pub fn tracked_order_ids(&self) -> Vec<BrokerOrderId> {
        let mut ids = self.owned_order_ids.iter().cloned().collect::<Vec<_>>();
        ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        ids
    }

    pub fn apply_order_event(&mut self, event: RuntimeOrderEvent) -> RuntimeCacheOrderApplyOutcome {
        self.apply_order_event_with_attribution(event, RuntimeOrderAttribution::ObservedAccountWide)
    }

    pub fn apply_owned_order_event(
        &mut self,
        event: RuntimeOrderEvent,
    ) -> RuntimeCacheOrderApplyOutcome {
        self.apply_order_event_with_attribution(event, RuntimeOrderAttribution::RuntimeOwned)
    }

    pub fn apply_adopted_bootstrap_order_event(
        &mut self,
        event: RuntimeOrderEvent,
    ) -> RuntimeCacheOrderApplyOutcome {
        self.apply_order_event_with_attribution(
            event,
            RuntimeOrderAttribution::AdoptedFromBootstrap,
        )
    }

    pub fn apply_order_event_with_attribution(
        &mut self,
        event: RuntimeOrderEvent,
        attribution: RuntimeOrderAttribution,
    ) -> RuntimeCacheOrderApplyOutcome {
        let order_id = event.order_id.clone();
        let lifecycle = event.lifecycle_classification().lifecycle;
        let mut lifecycle_blocker = (lifecycle == RuntimeOrderEventLifecycle::Unknown)
            .then_some(RuntimeCacheLifecycleBlocker::UnknownOrderLifecycle);
        if attribution == RuntimeOrderAttribution::UnknownOrOrphan && lifecycle_blocker.is_none() {
            lifecycle_blocker = Some(RuntimeCacheLifecycleBlocker::UnknownOrOrphanOwnership);
        }
        let disposition = match self.orders.get(&order_id) {
            Some(existing) if existing == &event => {
                RuntimeCacheApplyDisposition::DuplicateIdempotent
            }
            Some(_) => RuntimeCacheApplyDisposition::Updated,
            None => RuntimeCacheApplyDisposition::Inserted,
        };

        if disposition != RuntimeCacheApplyDisposition::DuplicateIdempotent {
            self.orders.insert(order_id.clone(), event);
        }
        self.observed_order_ids.insert(order_id.clone());
        if attribution.is_owned() {
            self.owned_order_ids.insert(order_id.clone());
        }

        let reconciled_pending_trade_count =
            if let Some(pending_trades) = self.pending_trades_by_order_id.remove(&order_id) {
                let count = pending_trades.len();
                for trade in pending_trades {
                    push_trade_dedup(
                        self.trades_by_order_id.entry(order_id.clone()).or_default(),
                        trade,
                    );
                }
                count
            } else {
                0
            };

        RuntimeCacheOrderApplyOutcome {
            broker_order_id_len: order_id.as_str().len(),
            attribution,
            disposition,
            lifecycle,
            lifecycle_blocker,
            tracked_order_count: self.owned_order_ids.len(),
            observed_order_count: self.observed_order_ids.len(),
            reconciled_pending_trade_count,
        }
    }

    pub fn apply_trade_event(&mut self, event: RuntimeTradeEvent) -> RuntimeCacheTradeApplyOutcome {
        let order_id = event.order_id.clone();
        let target = if self.owned_order_ids.contains(&order_id) {
            RuntimeTradeCacheTarget::OwnedOrder
        } else if self.orders.contains_key(&order_id) || self.observed_order_ids.contains(&order_id)
        {
            RuntimeTradeCacheTarget::KnownOrder
        } else {
            RuntimeTradeCacheTarget::PendingOrderEvent
        };
        let trades = match target {
            RuntimeTradeCacheTarget::OwnedOrder | RuntimeTradeCacheTarget::KnownOrder => {
                self.trades_by_order_id.entry(order_id.clone()).or_default()
            }
            RuntimeTradeCacheTarget::PendingOrderEvent => self
                .pending_trades_by_order_id
                .entry(order_id.clone())
                .or_default(),
        };
        let disposition = if push_trade_dedup(trades, event) {
            RuntimeCacheApplyDisposition::Inserted
        } else {
            RuntimeCacheApplyDisposition::DuplicateIdempotent
        };

        RuntimeCacheTradeApplyOutcome {
            broker_order_id_len: order_id.as_str().len(),
            target,
            disposition,
        }
    }

    pub fn set_pending(
        &mut self,
        path: RuntimePendingPath,
        pending: RuntimePendingRequestIdentity,
    ) {
        match path {
            RuntimePendingPath::Entry => self.pending_entry = Some(pending),
            RuntimePendingPath::Exit => self.pending_exit = Some(pending),
        }
    }

    pub fn apply_ack_to_pending_path(
        &mut self,
        path: RuntimePendingPath,
        ack: &CommandAck,
    ) -> Option<RuntimeAckLifecycleDecision> {
        let slot = match path {
            RuntimePendingPath::Entry => &mut self.pending_entry,
            RuntimePendingPath::Exit => &mut self.pending_exit,
        };
        let decision = slot.as_ref().map(|pending| pending.evaluate_ack(ack))?;
        if decision.pending_disposition == RuntimeAckPendingDisposition::ClearPending {
            *slot = None;
        }
        Some(decision)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePendingPath {
    Entry,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeOrderAttribution {
    RuntimeOwned,
    AdoptedFromBootstrap,
    ObservedAccountWide,
    UnknownOrOrphan,
}

impl RuntimeOrderAttribution {
    fn is_owned(self) -> bool {
        matches!(
            self,
            RuntimeOrderAttribution::RuntimeOwned | RuntimeOrderAttribution::AdoptedFromBootstrap
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCacheApplyDisposition {
    Inserted,
    Updated,
    DuplicateIdempotent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCacheLifecycleBlocker {
    UnknownOrderLifecycle,
    UnknownOrOrphanOwnership,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTradeCacheTarget {
    OwnedOrder,
    KnownOrder,
    PendingOrderEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCacheOrderApplyOutcome {
    pub broker_order_id_len: usize,
    pub attribution: RuntimeOrderAttribution,
    pub disposition: RuntimeCacheApplyDisposition,
    pub lifecycle: RuntimeOrderEventLifecycle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_blocker: Option<RuntimeCacheLifecycleBlocker>,
    pub tracked_order_count: usize,
    pub observed_order_count: usize,
    pub reconciled_pending_trade_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCacheTradeApplyOutcome {
    pub broker_order_id_len: usize,
    pub target: RuntimeTradeCacheTarget,
    pub disposition: RuntimeCacheApplyDisposition,
}

fn group_trades_by_order_id(
    trades: Vec<RuntimeTradeEvent>,
) -> HashMap<BrokerOrderId, Vec<RuntimeTradeEvent>> {
    let mut grouped = HashMap::<BrokerOrderId, Vec<RuntimeTradeEvent>>::new();
    for trade in trades {
        grouped
            .entry(trade.order_id.clone())
            .or_default()
            .push(trade);
    }
    grouped
}

fn push_trade_dedup(trades: &mut Vec<RuntimeTradeEvent>, trade: RuntimeTradeEvent) -> bool {
    if trades
        .iter()
        .any(|existing| existing.trade_id == trade.trade_id && existing.order_id == trade.order_id)
    {
        false
    } else {
        trades.push(trade);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request_id(value: &str) -> StrategyRequestId {
        StrategyRequestId::new(uuid::Uuid::parse_str(value).expect("uuid"))
    }

    fn ack(
        request_id: StrategyRequestId,
        client_order_id: Option<ClientOrderId>,
        broker_order_id: Option<BrokerOrderId>,
        status: CommandAckStatus,
    ) -> CommandAck {
        CommandAck {
            request_id,
            client_order_id,
            broker_order_id,
            status,
            reason: None,
            received_ts: DateTime::parse_from_rfc3339("2026-07-07T09:10:00Z")
                .expect("ts")
                .with_timezone(&Utc),
        }
    }

    fn pending(request_id: StrategyRequestId) -> RuntimePendingRequestIdentity {
        RuntimePendingRequestIdentity {
            request_id,
            client_order_id: None,
            broker_order_id: None,
        }
    }

    #[test]
    fn old_alor_numeric_order_id_deserializes_as_broker_order_id() {
        let order = serde_json::from_str::<RuntimeOrderEvent>(
            r#"{"order_id":2033126389943253218,"status":"working","symbol":"IMOEXF"}"#,
        )
        .expect("legacy numeric order id imports");

        assert_eq!(order.order_id.as_str(), "2033126389943253218");
        assert_eq!(order.status.as_deref(), Some("working"));
    }

    #[test]
    fn finam_string_broker_order_id_serializes_deserializes_exact() {
        let order = serde_json::from_str::<RuntimeOrderEvent>(
            r#"{"order_id":"FINAM-ORDER-ABC-001","status":"working"}"#,
        )
        .expect("native string id imports");

        assert_eq!(order.order_id.as_str(), "FINAM-ORDER-ABC-001");

        let serialized = serde_json::to_string(&order).expect("order serializes");
        assert!(serialized.contains(r#""order_id":"FINAM-ORDER-ABC-001""#));

        let roundtrip =
            serde_json::from_str::<RuntimeOrderEvent>(&serialized).expect("order roundtrips");
        assert_eq!(roundtrip.order_id, order.order_id);
    }

    #[test]
    fn empty_broker_order_id_rejected_at_runtime_state_boundary() {
        serde_json::from_str::<RuntimeOrderEvent>(r#"{"order_id":""}"#)
            .expect_err("empty order event id rejected");
        serde_json::from_str::<RuntimeStateSnapshot>(r#"{"known_order_ids":[""]}"#)
            .expect_err("empty known order id rejected");
        serde_json::from_str::<RuntimeCommandAckDto>(
            r#"{
                "request_id":"00000000-0000-4000-8000-000000000001",
                "broker_order_id_str":"",
                "status":"Submitted",
                "received_ts":"2026-07-07T09:10:00Z"
            }"#,
        )
        .expect_err("empty ack broker id rejected");
    }

    #[test]
    fn old_state_snapshot_reads_numeric_ids_and_can_write_v2_markers() {
        let state = serde_json::from_str::<RuntimeStateSnapshot>(
            r#"{
                "schema_version":1,
                "orders":{
                    "123":{"order_id":123,"status":"working","symbol":"IMOEXF"}
                },
                "known_order_ids":[123,456],
                "pending_entry_request_id":"00000000-0000-4000-8000-000000000010",
                "deferred_entry_state":"waiting_next_bar"
            }"#,
        )
        .expect("old numeric state snapshot imports");

        assert_eq!(
            state
                .orders
                .get(&BrokerOrderId::new("123"))
                .expect("order key imported")
                .order_id
                .as_str(),
            "123"
        );
        assert_eq!(
            state
                .known_order_ids
                .iter()
                .map(BrokerOrderId::as_str)
                .collect::<Vec<_>>(),
            vec!["123", "456"]
        );
        assert_eq!(
            state.pending_entry_request_id,
            Some(request_id("00000000-0000-4000-8000-000000000010"))
        );
        assert_eq!(
            state.deferred_entry_state.as_deref(),
            Some("waiting_next_bar")
        );

        let migrated = state.migrated_to_v2();
        let serialized = serde_json::to_value(&migrated).expect("v2 state serializes");
        assert_eq!(
            serialized["schema_version"],
            serde_json::json!(RUNTIME_STATE_SCHEMA_VERSION_V2)
        );
        assert_eq!(
            serialized["broker_order_id_encoding"],
            serde_json::json!(BROKER_ORDER_ID_ENCODING)
        );
        assert_eq!(
            serialized["legacy_alor_numeric_order_id_import"],
            serde_json::json!(LEGACY_ALOR_NUMERIC_ORDER_ID_IMPORT)
        );
    }

    #[test]
    fn runtime_state_orders_map_key_must_match_order_event_id() {
        let state = serde_json::from_str::<RuntimeStateSnapshot>(
            r#"{
                "orders":{
                    "123":{"order_id":"456","status":"working"}
                }
            }"#,
        )
        .expect("mismatched state is syntactically readable");

        assert_eq!(
            state
                .validate_for_runtime_restore()
                .expect_err("map key mismatch is rejected"),
            RuntimeStateValidationError::OrderMapKeyMismatch {
                map_name: "orders",
                key_len: 3,
                payload_order_id_len: 3,
            }
        );
    }

    #[test]
    fn bootstrap_working_orders_key_must_match_order_event_id() {
        let snapshot = serde_json::from_str::<RuntimeBootstrapSnapshotDto>(
            r#"{
                "working_orders":{
                    "123":{"order_id":"456","status":"working"}
                }
            }"#,
        )
        .expect("mismatched bootstrap is syntactically readable");

        assert_eq!(
            snapshot
                .validate_for_bootstrap()
                .expect_err("working_orders key mismatch is rejected"),
            RuntimeStateValidationError::OrderMapKeyMismatch {
                map_name: "working_orders",
                key_len: 3,
                payload_order_id_len: 3,
            }
        );
    }

    #[test]
    fn working_orders_strategy_key_must_match_order_event_id() {
        let snapshot = serde_json::from_str::<RuntimeBootstrapSnapshotDto>(
            r#"{
                "working_orders_strategy":{
                    "123":{"order_id":"456","status":"working"}
                }
            }"#,
        )
        .expect("mismatched bootstrap is syntactically readable");

        assert_eq!(
            snapshot
                .validate_for_bootstrap()
                .expect_err("working_orders_strategy key mismatch is rejected"),
            RuntimeStateValidationError::OrderMapKeyMismatch {
                map_name: "working_orders_strategy",
                key_len: 3,
                payload_order_id_len: 3,
            }
        );
    }

    #[test]
    fn known_order_ids_cannot_contain_empty_zero_negative_null_or_duplicates() {
        for payload in [
            r#"{"known_order_ids":[""]}"#,
            r#"{"known_order_ids":[0]}"#,
            r#"{"known_order_ids":[-1]}"#,
            r#"{"known_order_ids":[null]}"#,
        ] {
            serde_json::from_str::<RuntimeStateSnapshot>(payload)
                .expect_err("invalid known order id rejected at serde boundary");
        }

        let duplicate =
            serde_json::from_str::<RuntimeStateSnapshot>(r#"{"known_order_ids":[123,"123"]}"#)
                .expect("duplicate is syntactically readable");

        assert_eq!(
            duplicate
                .validate_for_runtime_restore()
                .expect_err("duplicate known id rejected"),
            RuntimeStateValidationError::DuplicateBrokerOrderId {
                field: "known_order_ids",
                id_len: 3,
            }
        );
    }

    #[test]
    fn known_order_id_missing_from_orders_blocks_readiness_without_losing_state() {
        let state = serde_json::from_str::<RuntimeStateSnapshot>(
            r#"{
                "orders":{
                    "123":{"order_id":123,"status":"working"}
                },
                "known_order_ids":[123,999],
                "pending_exit_request_id":"00000000-0000-4000-8000-000000000011",
                "deferred_exit_state":"waiting_fill"
            }"#,
        )
        .expect("state imports");

        let validated = state
            .validate_for_runtime_restore()
            .expect("map is consistent");

        assert!(validated.manual_intervention_required());
        assert_eq!(
            validated.readiness_blockers,
            vec![RuntimeStateReadinessBlocker {
                kind: RuntimeStateReadinessBlockerKind::KnownOrderIdMissingFromOrders,
                broker_order_id_len: 3,
            }]
        );
        assert_eq!(
            validated.snapshot.pending_exit_request_id,
            Some(request_id("00000000-0000-4000-8000-000000000011"))
        );
        assert_eq!(
            validated.snapshot.deferred_exit_state.as_deref(),
            Some("waiting_fill")
        );
    }

    #[test]
    fn new_state_serializes_broker_order_id_keys_as_exact_strings() {
        let state = serde_json::from_str::<RuntimeStateSnapshot>(
            r#"{
                "orders":{
                    "FINAM-ORDER-ABC-001":{"order_id":"FINAM-ORDER-ABC-001","status":"working"}
                },
                "known_order_ids":["FINAM-ORDER-ABC-001"]
            }"#,
        )
        .expect("state imports")
        .validate_for_runtime_restore()
        .expect("state validates")
        .snapshot;

        let serialized = serde_json::to_string(&state).expect("state serializes");
        assert!(serialized.contains(r#""FINAM-ORDER-ABC-001":{"order_id":"FINAM-ORDER-ABC-001""#));
    }

    #[test]
    fn runtime_command_ack_imports_legacy_ids_without_replacing_request_id() {
        let ack = serde_json::from_str::<RuntimeCommandAckDto>(
            r#"{
                "request_id":"00000000-0000-4000-8000-000000000020",
                "client_order_id":"CID000000000000020",
                "broker_order_id":789,
                "status":"Submitted",
                "received_ts":"2026-07-07T09:10:00Z"
            }"#,
        )
        .expect("legacy numeric ack broker id imports");

        assert_eq!(
            ack.request_id,
            request_id("00000000-0000-4000-8000-000000000020")
        );
        assert_eq!(
            ack.client_order_id.as_ref().map(ClientOrderId::as_str),
            Some("CID000000000000020")
        );
        assert_eq!(
            ack.broker_order_id.as_ref().map(BrokerOrderId::as_str),
            Some("789")
        );

        let command_ack = CommandAck::from(ack);
        assert_eq!(
            command_ack.request_id,
            request_id("00000000-0000-4000-8000-000000000020")
        );
        assert_eq!(
            command_ack
                .broker_order_id
                .as_ref()
                .map(BrokerOrderId::as_str),
            Some("789")
        );
    }

    #[test]
    fn runtime_command_ack_rejects_conflicting_primary_and_legacy_string_ids() {
        serde_json::from_str::<RuntimeCommandAckDto>(
            r#"{
                "request_id":"00000000-0000-4000-8000-000000000021",
                "broker_order_id":789,
                "broker_order_id_str":"790",
                "status":"Submitted",
                "received_ts":"2026-07-07T09:10:00Z"
            }"#,
        )
        .expect_err("conflicting broker ids rejected");
    }

    #[test]
    fn client_order_id_does_not_replace_strategy_request_id() {
        let ack = serde_json::from_str::<RuntimeCommandAckDto>(
            r#"{
                "request_id":"00000000-0000-4000-8000-000000000030",
                "client_order_id":"CID000000000000999",
                "status":"Accepted",
                "received_ts":"2026-07-07T09:10:00Z"
            }"#,
        )
        .expect("ack imports");

        assert_eq!(
            ack.request_id,
            request_id("00000000-0000-4000-8000-000000000030")
        );
        assert_ne!(
            ack.client_order_id.map(|value| value.to_string()),
            Some(ack.request_id.to_string())
        );
    }

    #[test]
    fn matching_strategy_request_id_ack_can_clear_matching_pending_path() {
        let request_id = request_id("00000000-0000-4000-8000-000000000040");
        let pending = RuntimePendingRequestIdentity {
            request_id,
            client_order_id: Some(ClientOrderId::new("CID000000000000040").expect("cid")),
            broker_order_id: None,
        };
        let ack = ack(
            request_id,
            Some(ClientOrderId::new("CID000000000000040").expect("cid")),
            Some(BrokerOrderId::new("FINAM-ORDER-040")),
            CommandAckStatus::Submitted,
        );

        let decision = pending.evaluate_ack(&ack);

        assert!(decision.request_id_matches);
        assert!(decision.client_order_id_matches);
        assert_eq!(
            decision.broker_order_id_state,
            RuntimeAckBrokerOrderIdState::Present
        );
        assert_eq!(
            decision.pending_disposition,
            RuntimeAckPendingDisposition::ClearPending
        );
        assert!(decision.issues.is_empty());
    }

    #[test]
    fn mismatched_strategy_request_id_ack_never_clears_pending_even_with_client_match() {
        let pending = RuntimePendingRequestIdentity {
            request_id: request_id("00000000-0000-4000-8000-000000000041"),
            client_order_id: Some(ClientOrderId::new("CID000000000000041").expect("cid")),
            broker_order_id: None,
        };
        let ack = ack(
            request_id("00000000-0000-4000-8000-000000000042"),
            Some(ClientOrderId::new("CID000000000000041").expect("cid")),
            None,
            CommandAckStatus::Rejected,
        );

        let decision = pending.evaluate_ack(&ack);

        assert!(!decision.request_id_matches);
        assert!(decision.client_order_id_matches);
        assert_eq!(
            decision.pending_disposition,
            RuntimeAckPendingDisposition::KeepPending
        );
        assert_eq!(
            decision.issues,
            vec![
                RuntimeAckLifecycleIssue::RequestIdMismatch,
                RuntimeAckLifecycleIssue::ClientOrderIdOnlyMatchDoesNotClearPending,
            ]
        );
    }

    #[test]
    fn broker_order_id_does_not_replace_strategy_request_id() {
        let pending = RuntimePendingRequestIdentity {
            request_id: request_id("00000000-0000-4000-8000-000000000043"),
            client_order_id: None,
            broker_order_id: Some(BrokerOrderId::new("FINAM-ORDER-043")),
        };
        let ack = ack(
            request_id("00000000-0000-4000-8000-000000000044"),
            None,
            Some(BrokerOrderId::new("FINAM-ORDER-043")),
            CommandAckStatus::Submitted,
        );

        let decision = pending.evaluate_ack(&ack);

        assert!(!decision.request_id_matches);
        assert!(decision.broker_order_id_matches);
        assert_eq!(
            decision.pending_disposition,
            RuntimeAckPendingDisposition::KeepPending
        );
        assert_eq!(
            decision.issues,
            vec![
                RuntimeAckLifecycleIssue::RequestIdMismatch,
                RuntimeAckLifecycleIssue::BrokerOrderIdOnlyMatchDoesNotClearPending,
            ]
        );
    }

    #[test]
    fn submitted_ack_missing_broker_order_id_is_marked_pending_broker_id() {
        let request_id = request_id("00000000-0000-4000-8000-000000000045");
        let pending = pending(request_id);
        let ack = ack(request_id, None, None, CommandAckStatus::Submitted);

        let decision = pending.evaluate_ack(&ack);

        assert!(decision.request_id_matches);
        assert_eq!(
            decision.broker_order_id_state,
            RuntimeAckBrokerOrderIdState::PendingBrokerOrderId
        );
        assert_eq!(
            decision.pending_disposition,
            RuntimeAckPendingDisposition::KeepPendingBrokerOrderId
        );
    }

    #[test]
    fn rejected_ack_may_omit_broker_order_id_when_request_matches() {
        let request_id = request_id("00000000-0000-4000-8000-000000000046");
        let pending = pending(request_id);
        let ack = ack(request_id, None, None, CommandAckStatus::Rejected);

        let decision = pending.evaluate_ack(&ack);

        assert!(decision.request_id_matches);
        assert_eq!(
            decision.broker_order_id_state,
            RuntimeAckBrokerOrderIdState::NotRequired
        );
        assert_eq!(
            decision.pending_disposition,
            RuntimeAckPendingDisposition::ClearPending
        );
    }

    #[test]
    fn error_ack_with_matching_request_id_does_not_clear_pending_by_default() {
        let request_id = request_id("00000000-0000-4000-8000-000000000050");
        let pending = pending(request_id);
        let ack = ack(request_id, None, None, CommandAckStatus::Error);

        let decision = pending.evaluate_ack(&ack);

        assert_eq!(
            decision.status_policy,
            RuntimeAckStatusPolicy::ManualInterventionRequired
        );
        assert_eq!(
            decision.pending_disposition,
            RuntimeAckPendingDisposition::KeepPending
        );
        assert_eq!(
            decision.issues,
            vec![RuntimeAckLifecycleIssue::AmbiguousErrorAckDoesNotClearPending]
        );
    }

    #[test]
    fn duplicate_ack_with_matching_request_id_requires_prior_outcome_before_clearing() {
        let request_id = request_id("00000000-0000-4000-8000-000000000051");
        let pending = pending(request_id);
        let ack = ack(request_id, None, None, CommandAckStatus::Duplicate);

        let decision = pending.evaluate_ack(&ack);

        assert_eq!(
            decision.status_policy,
            RuntimeAckStatusPolicy::RequiresPriorOutcome
        );
        assert_eq!(
            decision.pending_disposition,
            RuntimeAckPendingDisposition::KeepPending
        );
        assert_eq!(
            decision.issues,
            vec![RuntimeAckLifecycleIssue::DuplicateAckRequiresPriorOutcome]
        );
    }

    #[test]
    fn expired_ack_requires_explicit_no_send_policy_before_clearing() {
        let request_id = request_id("00000000-0000-4000-8000-000000000052");
        let pending = pending(request_id);
        let ack_without_proof = ack(request_id, None, None, CommandAckStatus::Expired);

        let decision = pending.evaluate_ack(&ack_without_proof);

        assert_eq!(
            decision.status_policy,
            RuntimeAckStatusPolicy::RequiresNoSendProof
        );
        assert_eq!(
            decision.pending_disposition,
            RuntimeAckPendingDisposition::KeepPending
        );
        assert_eq!(
            decision.issues,
            vec![RuntimeAckLifecycleIssue::ExpiredAckRequiresNoSendProof]
        );

        let ack_with_no_send_proof = CommandAck {
            reason: Some(CommandAckReason::new(CommandAckReasonCode::ExpiredCommand)),
            ..ack_without_proof
        };
        let decision = pending.evaluate_ack(&ack_with_no_send_proof);

        assert_eq!(decision.status_policy, RuntimeAckStatusPolicy::ClearPending);
        assert_eq!(
            decision.pending_disposition,
            RuntimeAckPendingDisposition::ClearPending
        );
        assert!(decision.issues.is_empty());
    }

    #[test]
    fn timeout_and_unknown_pending_ack_keep_pending() {
        for status in [CommandAckStatus::Timeout, CommandAckStatus::UnknownPending] {
            let request_id = request_id("00000000-0000-4000-8000-000000000053");
            let pending = pending(request_id);
            let ack = ack(request_id, None, None, status);

            let decision = pending.evaluate_ack(&ack);

            assert_eq!(decision.status_policy, RuntimeAckStatusPolicy::KeepPending);
            assert_eq!(
                decision.pending_disposition,
                RuntimeAckPendingDisposition::KeepPending
            );
            assert!(decision.issues.is_empty());
        }
    }

    #[test]
    fn order_and_trade_events_preserve_exact_broker_order_id_and_classify_lifecycle() {
        let order = serde_json::from_str::<RuntimeOrderEvent>(
            r#"{"order_id":"FINAM/ORDER:EXACT-047","status":"working","symbol":"IMOEXF"}"#,
        )
        .expect("order imports");
        let trade = serde_json::from_str::<RuntimeTradeEvent>(
            r#"{"trade_id":"FINAM-TRADE-047","order_id":"FINAM/ORDER:EXACT-047","symbol":"IMOEXF"}"#,
        )
        .expect("trade imports");

        assert_eq!(order.order_id.as_str(), "FINAM/ORDER:EXACT-047");
        assert_eq!(trade.order_id.as_str(), "FINAM/ORDER:EXACT-047");
        assert_eq!(
            order.lifecycle_classification().lifecycle,
            RuntimeOrderEventLifecycle::Active
        );

        let terminal = RuntimeOrderEvent {
            status: Some("Filled".to_string()),
            ..order
        };
        assert_eq!(
            terminal.lifecycle_classification().lifecycle,
            RuntimeOrderEventLifecycle::Terminal
        );
    }

    #[test]
    fn broker_event_before_ack_is_representable_without_corrupting_pending_state() {
        let request_id = request_id("00000000-0000-4000-8000-000000000048");
        let pending = RuntimePendingRequestIdentity {
            request_id,
            client_order_id: Some(ClientOrderId::new("CID000000000000048").expect("cid")),
            broker_order_id: None,
        };
        let order = RuntimeOrderEvent {
            order_id: BrokerOrderId::new("FINAM-ORDER-BEFORE-ACK-048"),
            client_order_id: Some(ClientOrderId::new("CID000000000000048").expect("cid")),
            symbol: Some("IMOEXF".to_string()),
            exchange: None,
            status: Some("working".to_string()),
            side: Some("buy".to_string()),
            order_type: Some("limit".to_string()),
            source_ts: None,
        };
        let ack = ack(
            request_id,
            order.client_order_id.clone(),
            None,
            CommandAckStatus::Accepted,
        );

        assert_eq!(
            order.lifecycle_classification().lifecycle,
            RuntimeOrderEventLifecycle::Active
        );
        assert_eq!(
            pending.evaluate_ack(&ack).pending_disposition,
            RuntimeAckPendingDisposition::KeepPendingBrokerOrderId
        );
    }

    #[test]
    fn duplicate_broker_events_are_classified_idempotent_at_dto_layer() {
        let mut deduplicator = RuntimeBrokerEventDeduplicator::default();
        let order = RuntimeOrderEvent {
            order_id: BrokerOrderId::new("FINAM-ORDER-DUP-049"),
            client_order_id: None,
            symbol: Some("IMOEXF".to_string()),
            exchange: None,
            status: Some("working".to_string()),
            side: None,
            order_type: None,
            source_ts: None,
        };
        let trade = RuntimeTradeEvent {
            trade_id: BrokerTradeId::new("FINAM-TRADE-DUP-049"),
            order_id: BrokerOrderId::new("FINAM-ORDER-DUP-049"),
            client_order_id: None,
            symbol: Some("IMOEXF".to_string()),
            exchange: None,
            side: None,
            source_ts: None,
        };

        assert_eq!(
            deduplicator.classify_order_event(&order),
            RuntimeBrokerEventReplayDisposition::New
        );
        assert_eq!(
            deduplicator.classify_order_event(&order),
            RuntimeBrokerEventReplayDisposition::DuplicateIdempotent
        );
        assert_eq!(
            deduplicator.classify_trade_event(&trade),
            RuntimeBrokerEventReplayDisposition::New
        );
        assert_eq!(
            deduplicator.classify_trade_event(&trade),
            RuntimeBrokerEventReplayDisposition::DuplicateIdempotent
        );
    }

    #[test]
    fn runtime_caches_orders_use_string_broker_order_id_and_lookup_exact() {
        let mut caches = RuntimeCaches::new();
        let order = RuntimeOrderEvent {
            order_id: BrokerOrderId::new("FINAM-ORDER-CACHE-060"),
            client_order_id: Some(ClientOrderId::new("CID000000000000060").expect("cid")),
            symbol: Some("IMOEXF".to_string()),
            exchange: None,
            status: Some("working".to_string()),
            side: Some("buy".to_string()),
            order_type: Some("limit".to_string()),
            source_ts: None,
        };

        let outcome = caches.apply_owned_order_event(order);

        assert_eq!(outcome.disposition, RuntimeCacheApplyDisposition::Inserted);
        assert_eq!(outcome.attribution, RuntimeOrderAttribution::RuntimeOwned);
        assert_eq!(outcome.lifecycle, RuntimeOrderEventLifecycle::Active);
        assert_eq!(
            caches
                .order(&BrokerOrderId::new("FINAM-ORDER-CACHE-060"))
                .expect("order cached")
                .order_id
                .as_str(),
            "FINAM-ORDER-CACHE-060"
        );
        assert_eq!(
            caches
                .tracked_order_ids()
                .iter()
                .map(BrokerOrderId::as_str)
                .collect::<Vec<_>>(),
            vec!["FINAM-ORDER-CACHE-060"]
        );
    }

    #[test]
    fn runtime_caches_import_legacy_numeric_alor_ids_as_decimal_strings() {
        let state = serde_json::from_str::<RuntimeStateSnapshot>(
            r#"{
                "orders":{
                    "2033126389943253218":{
                        "order_id":2033126389943253218,
                        "status":"working",
                        "symbol":"IMOEXF"
                    }
                },
                "known_order_ids":[2033126389943253218]
            }"#,
        )
        .expect("legacy state imports");
        let caches = RuntimeCaches::from_validated_state(
            state
                .validate_for_runtime_restore()
                .expect("legacy state validates"),
        );

        assert!(caches
            .order(&BrokerOrderId::new("2033126389943253218"))
            .is_some());
        assert_eq!(
            caches
                .tracked_order_ids()
                .iter()
                .map(BrokerOrderId::as_str)
                .collect::<Vec<_>>(),
            vec!["2033126389943253218"]
        );
        assert_eq!(
            caches
                .observed_order_ids
                .iter()
                .map(BrokerOrderId::as_str)
                .collect::<Vec<_>>(),
            vec!["2033126389943253218"]
        );
    }

    #[test]
    fn from_validated_state_tracks_known_order_ids_only_not_all_observed_orders() {
        let state = serde_json::from_str::<RuntimeStateSnapshot>(
            r#"{
                "orders":{
                    "KNOWN-ORDER-065":{"order_id":"KNOWN-ORDER-065","status":"working"},
                    "OBSERVED-ORDER-065":{"order_id":"OBSERVED-ORDER-065","status":"working"}
                },
                "known_order_ids":["KNOWN-ORDER-065"]
            }"#,
        )
        .expect("state imports");
        let caches = RuntimeCaches::from_validated_state(
            state
                .validate_for_runtime_restore()
                .expect("state validates"),
        );

        assert_eq!(
            caches
                .tracked_order_ids()
                .iter()
                .map(BrokerOrderId::as_str)
                .collect::<Vec<_>>(),
            vec!["KNOWN-ORDER-065"]
        );
        assert!(caches
            .observed_order_ids
            .contains(&BrokerOrderId::new("OBSERVED-ORDER-065")));
        assert!(!caches
            .owned_order_ids
            .contains(&BrokerOrderId::new("OBSERVED-ORDER-065")));
    }

    #[test]
    fn duplicate_order_event_is_idempotent_and_does_not_create_duplicate_cache_entries() {
        let mut caches = RuntimeCaches::new();
        let order = RuntimeOrderEvent {
            order_id: BrokerOrderId::new("FINAM-ORDER-DUP-CACHE-061"),
            client_order_id: None,
            symbol: Some("IMOEXF".to_string()),
            exchange: None,
            status: Some("working".to_string()),
            side: None,
            order_type: None,
            source_ts: None,
        };

        let first = caches.apply_order_event(order.clone());
        let duplicate = caches.apply_order_event(order);

        assert_eq!(first.disposition, RuntimeCacheApplyDisposition::Inserted);
        assert_eq!(
            first.attribution,
            RuntimeOrderAttribution::ObservedAccountWide
        );
        assert_eq!(
            duplicate.disposition,
            RuntimeCacheApplyDisposition::DuplicateIdempotent
        );
        assert_eq!(caches.orders.len(), 1);
        assert_eq!(caches.observed_order_ids.len(), 1);
        assert_eq!(caches.owned_order_ids.len(), 0);
    }

    #[test]
    fn unknown_lifecycle_order_event_stays_blocking_not_terminal_clean() {
        let mut caches = RuntimeCaches::new();
        let outcome = caches.apply_order_event_with_attribution(
            RuntimeOrderEvent {
                order_id: BrokerOrderId::new("FINAM-ORDER-UNKNOWN-062"),
                client_order_id: None,
                symbol: Some("IMOEXF".to_string()),
                exchange: None,
                status: Some("broker_new_status".to_string()),
                side: None,
                order_type: None,
                source_ts: None,
            },
            RuntimeOrderAttribution::UnknownOrOrphan,
        );

        assert_eq!(
            outcome.attribution,
            RuntimeOrderAttribution::UnknownOrOrphan
        );
        assert_eq!(outcome.lifecycle, RuntimeOrderEventLifecycle::Unknown);
        assert_eq!(
            outcome.lifecycle_blocker,
            Some(RuntimeCacheLifecycleBlocker::UnknownOrderLifecycle)
        );
        assert_ne!(outcome.lifecycle, RuntimeOrderEventLifecycle::Terminal);
        assert!(caches.tracked_order_ids().is_empty());
    }

    #[test]
    fn observed_order_event_does_not_become_tracked_order_id() {
        let mut caches = RuntimeCaches::new();
        let outcome = caches.apply_order_event(RuntimeOrderEvent {
            order_id: BrokerOrderId::new("FINAM-ORDER-OBSERVED-066"),
            client_order_id: None,
            symbol: Some("IMOEXF".to_string()),
            exchange: None,
            status: Some("working".to_string()),
            side: None,
            order_type: None,
            source_ts: None,
        });

        assert_eq!(
            outcome.attribution,
            RuntimeOrderAttribution::ObservedAccountWide
        );
        assert_eq!(outcome.tracked_order_count, 0);
        assert_eq!(outcome.observed_order_count, 1);
        assert!(caches.tracked_order_ids().is_empty());
        assert!(caches
            .observed_order_ids
            .contains(&BrokerOrderId::new("FINAM-ORDER-OBSERVED-066")));
    }

    #[test]
    fn unknown_or_orphan_order_event_sets_blocker_and_is_not_owned_even_when_status_known() {
        let mut caches = RuntimeCaches::new();
        let outcome = caches.apply_order_event_with_attribution(
            RuntimeOrderEvent {
                order_id: BrokerOrderId::new("FINAM-ORDER-ORPHAN-069"),
                client_order_id: None,
                symbol: Some("IMOEXF".to_string()),
                exchange: None,
                status: Some("working".to_string()),
                side: None,
                order_type: None,
                source_ts: None,
            },
            RuntimeOrderAttribution::UnknownOrOrphan,
        );

        assert_eq!(outcome.lifecycle, RuntimeOrderEventLifecycle::Active);
        assert_eq!(
            outcome.lifecycle_blocker,
            Some(RuntimeCacheLifecycleBlocker::UnknownOrOrphanOwnership)
        );
        assert!(caches.tracked_order_ids().is_empty());
    }

    #[test]
    fn adopted_bootstrap_order_event_becomes_tracked_order_id_only_when_explicit() {
        let mut caches = RuntimeCaches::new();
        let outcome = caches.apply_adopted_bootstrap_order_event(RuntimeOrderEvent {
            order_id: BrokerOrderId::new("FINAM-ORDER-ADOPTED-067"),
            client_order_id: None,
            symbol: Some("IMOEXF".to_string()),
            exchange: None,
            status: Some("working".to_string()),
            side: None,
            order_type: None,
            source_ts: None,
        });

        assert_eq!(
            outcome.attribution,
            RuntimeOrderAttribution::AdoptedFromBootstrap
        );
        assert_eq!(
            caches
                .tracked_order_ids()
                .iter()
                .map(BrokerOrderId::as_str)
                .collect::<Vec<_>>(),
            vec!["FINAM-ORDER-ADOPTED-067"]
        );
    }

    #[test]
    fn trade_event_before_order_event_is_pending_then_reconciled_by_exact_broker_id() {
        let mut caches = RuntimeCaches::new();
        let trade = RuntimeTradeEvent {
            trade_id: BrokerTradeId::new("FINAM-TRADE-CACHE-063"),
            order_id: BrokerOrderId::new("FINAM-ORDER-CACHE-063"),
            client_order_id: None,
            symbol: Some("IMOEXF".to_string()),
            exchange: None,
            side: Some("buy".to_string()),
            source_ts: None,
        };

        let trade_outcome = caches.apply_trade_event(trade.clone());

        assert_eq!(
            trade_outcome.target,
            RuntimeTradeCacheTarget::PendingOrderEvent
        );
        assert_eq!(
            caches
                .pending_trades_by_order_id
                .get(&BrokerOrderId::new("FINAM-ORDER-CACHE-063"))
                .expect("pending trade stored")
                .len(),
            1
        );
        assert_eq!(
            caches.apply_trade_event(trade).disposition,
            RuntimeCacheApplyDisposition::DuplicateIdempotent
        );

        let order_outcome = caches.apply_owned_order_event(RuntimeOrderEvent {
            order_id: BrokerOrderId::new("FINAM-ORDER-CACHE-063"),
            client_order_id: None,
            symbol: Some("IMOEXF".to_string()),
            exchange: None,
            status: Some("filled".to_string()),
            side: Some("buy".to_string()),
            order_type: Some("market".to_string()),
            source_ts: None,
        });

        assert_eq!(order_outcome.reconciled_pending_trade_count, 1);
        assert!(!caches
            .pending_trades_by_order_id
            .contains_key(&BrokerOrderId::new("FINAM-ORDER-CACHE-063")));
        assert_eq!(
            caches
                .trades_by_order_id
                .get(&BrokerOrderId::new("FINAM-ORDER-CACHE-063"))
                .expect("trade reconciled")
                .len(),
            1
        );
    }

    #[test]
    fn trade_for_observed_order_is_not_strategy_attributed_without_ownership() {
        let mut caches = RuntimeCaches::new();
        caches.apply_order_event(RuntimeOrderEvent {
            order_id: BrokerOrderId::new("FINAM-ORDER-OBSERVED-068"),
            client_order_id: None,
            symbol: Some("IMOEXF".to_string()),
            exchange: None,
            status: Some("working".to_string()),
            side: None,
            order_type: None,
            source_ts: None,
        });

        let outcome = caches.apply_trade_event(RuntimeTradeEvent {
            trade_id: BrokerTradeId::new("FINAM-TRADE-OBSERVED-068"),
            order_id: BrokerOrderId::new("FINAM-ORDER-OBSERVED-068"),
            client_order_id: None,
            symbol: Some("IMOEXF".to_string()),
            exchange: None,
            side: Some("buy".to_string()),
            source_ts: None,
        });

        assert_eq!(outcome.target, RuntimeTradeCacheTarget::KnownOrder);
        assert!(caches.tracked_order_ids().is_empty());
        assert!(!caches
            .owned_order_ids
            .contains(&BrokerOrderId::new("FINAM-ORDER-OBSERVED-068")));
    }

    #[test]
    fn runtime_cache_pending_ack_helper_respects_stage_2b4a_policy() {
        let request_id = request_id("00000000-0000-4000-8000-000000000064");
        let mut caches = RuntimeCaches::new();
        caches.set_pending(RuntimePendingPath::Entry, pending(request_id));

        let error_ack = ack(request_id, None, None, CommandAckStatus::Error);
        let decision = caches
            .apply_ack_to_pending_path(RuntimePendingPath::Entry, &error_ack)
            .expect("pending entry evaluated");

        assert_eq!(
            decision.status_policy,
            RuntimeAckStatusPolicy::ManualInterventionRequired
        );
        assert_eq!(
            decision.pending_disposition,
            RuntimeAckPendingDisposition::KeepPending
        );
        assert!(caches.pending_entry.is_some());

        let rejected_ack = ack(request_id, None, None, CommandAckStatus::Rejected);
        let decision = caches
            .apply_ack_to_pending_path(RuntimePendingPath::Entry, &rejected_ack)
            .expect("pending entry evaluated");

        assert_eq!(
            decision.pending_disposition,
            RuntimeAckPendingDisposition::ClearPending
        );
        assert!(caches.pending_entry.is_none());
    }
}
