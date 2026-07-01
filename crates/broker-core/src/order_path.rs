use std::collections::HashMap;
use std::fmt::Write;
use std::fs::{self, File, OpenOptions};
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::account::AccountId;
use crate::command::{CancelOrder, CommandAck, CommandAckReason, CommandAckStatus, PlaceOrder};
use crate::ids::{BrokerOrderId, ClientOrderId, StrategyRequestId};
use crate::instrument::{InstrumentId, Price, Quantity};
use crate::order::{OrderSide, OrderType, RedactedValueFingerprint, TimeInForce};

const SQLITE_ORDER_PATH_SCHEMA_VERSION: i64 = 1;

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
    SubmittedPendingBrokerOrderId,
    TimeoutUnknownPending,
    RecoveredByClientOrderId,
    BrokerRejected,
    CancelRequested,
    CancelSubmitted,
    CancelTimeoutUnknownPending,
    CancelRecoveredTerminal,
    Terminal,
    ManualInterventionRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderPathEvent {
    LocalReject,
    BeginSubmit,
    SubmitAccepted,
    SubmitAcceptedWithoutBrokerOrderId,
    SubmitTimedOut,
    RecoverByClientOrderId,
    RequireManualIntervention,
    BrokerReject,
    RequestCancel,
    CancelAccepted,
    CancelRejected,
    CancelTimedOut,
    RecoverCancelTerminal,
    MarkTerminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderPathErrorKind {
    LocalValidation,
    BrokerRejected,
    TransportTimeout,
    RateLimited,
    BrokerMaintenance,
    ResponseDecodeError,
    Unauthorized,
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
            OrderPathEvent::SubmitAcceptedWithoutBrokerOrderId => {
                self.last_ack_status = Some(CommandAckStatus::UnknownPending);
                self.last_error_kind = Some(OrderPathErrorKind::ReconciliationRequired);
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
            OrderPathEvent::CancelRejected => {
                self.last_ack_status = Some(CommandAckStatus::Rejected);
                self.last_error_kind = Some(OrderPathErrorKind::BrokerRejected);
            }
            OrderPathEvent::CancelTimedOut => {
                self.last_ack_status = Some(CommandAckStatus::Timeout);
                self.last_error_kind = Some(OrderPathErrorKind::TransportTimeout);
            }
            OrderPathEvent::RecoverCancelTerminal => {
                self.last_ack_status = Some(CommandAckStatus::Recovered);
                self.last_reconciliation_source =
                    Some(OrderPathReconciliationSource::OrderSnapshot);
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
        reason: Option<CommandAckReason>,
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
        (S::SubmitInFlight, E::SubmitAcceptedWithoutBrokerOrderId) => {
            Some(S::SubmittedPendingBrokerOrderId)
        }
        (S::SubmitInFlight, E::SubmitTimedOut) => Some(S::TimeoutUnknownPending),
        (S::SubmitInFlight, E::RequireManualIntervention) => Some(S::ManualInterventionRequired),
        (S::TimeoutUnknownPending, E::RecoverByClientOrderId) => Some(S::RecoveredByClientOrderId),
        (S::SubmittedPendingBrokerOrderId, E::RecoverByClientOrderId) => {
            Some(S::RecoveredByClientOrderId)
        }
        (S::SubmittedPendingBrokerOrderId, E::RequireManualIntervention) => {
            Some(S::ManualInterventionRequired)
        }
        (S::TimeoutUnknownPending, E::RequireManualIntervention) => {
            Some(S::ManualInterventionRequired)
        }
        (S::SubmitInFlight, E::BrokerReject) => Some(S::BrokerRejected),
        (S::Submitted, E::BrokerReject) => Some(S::BrokerRejected),
        (S::Submitted, E::RequestCancel) | (S::RecoveredByClientOrderId, E::RequestCancel) => {
            Some(S::CancelRequested)
        }
        (S::CancelRequested, E::CancelAccepted) => Some(S::CancelSubmitted),
        (S::CancelRequested, E::CancelRejected) | (S::CancelSubmitted, E::CancelRejected) => {
            Some(S::ManualInterventionRequired)
        }
        (S::CancelRequested, E::RequireManualIntervention)
        | (S::CancelSubmitted, E::RequireManualIntervention) => Some(S::ManualInterventionRequired),
        (S::CancelRequested, E::CancelTimedOut) | (S::CancelSubmitted, E::CancelTimedOut) => {
            Some(S::CancelTimeoutUnknownPending)
        }
        (S::CancelTimeoutUnknownPending, E::RecoverCancelTerminal) => {
            Some(S::CancelRecoveredTerminal)
        }
        (S::CancelTimeoutUnknownPending, E::RequireManualIntervention) => {
            Some(S::ManualInterventionRequired)
        }
        (S::Submitted, E::MarkTerminal)
        | (S::RecoveredByClientOrderId, E::MarkTerminal)
        | (S::CancelRequested, E::MarkTerminal)
        | (S::CancelSubmitted, E::MarkTerminal)
        | (S::CancelTimeoutUnknownPending, E::MarkTerminal)
        | (S::CancelRecoveredTerminal, E::MarkTerminal)
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
    #[error("duplicate broker order id: {0}")]
    DuplicateBrokerOrderId(BrokerOrderId),
    #[error("order-path record not found: {0}")]
    RecordNotFound(StrategyRequestId),
    #[error("client_order_id cannot be changed for request: {0}")]
    ClientOrderIdChanged(StrategyRequestId),
    #[error("broker_order_id cannot be changed for request: {0}")]
    BrokerOrderIdChanged(StrategyRequestId),
    #[error("terminal order-path state cannot be overwritten for request: {0}")]
    TerminalStateOverwrite(StrategyRequestId),
    #[error("order-path updated timestamp regressed for request: {0}")]
    UpdatedTimestampRegressed(StrategyRequestId),
    #[error("order-path store io error: {0}")]
    Io(String),
    #[error("order-path store writer lock unavailable: {0}")]
    WriterLockUnavailable(String),
    #[error("order-path sqlite schema version mismatch: expected {expected}, found {found}")]
    SchemaVersionMismatch { expected: i64, found: i64 },
    #[error("order-path sqlite store error: {0}")]
    Sqlite(String),
    #[error("order-path store serialization error: {0}")]
    Serialization(String),
}

impl OrderPathStoreError {
    pub fn operator_disarm_signal(&self) -> OperatorDisarmSignal {
        match self {
            Self::WriterLockUnavailable(_) => OperatorDisarmSignal::OrderPathStoreLockUncertain,
            Self::SchemaVersionMismatch { .. } => {
                OperatorDisarmSignal::OrderPathStoreMigrationMismatch
            }
            Self::DuplicateStrategyRequestId(_)
            | Self::DuplicateClientOrderId(_)
            | Self::DuplicateBrokerOrderId(_)
            | Self::RecordNotFound(_)
            | Self::ClientOrderIdChanged(_)
            | Self::BrokerOrderIdChanged(_)
            | Self::TerminalStateOverwrite(_)
            | Self::UpdatedTimestampRegressed(_)
            | Self::Io(_)
            | Self::Sqlite(_)
            | Self::Serialization(_) => OperatorDisarmSignal::OrderPathStoreUnavailable,
        }
    }
}

pub trait OrderPathStore {
    fn insert_intent(&mut self, record: OrderPathRecord) -> Result<(), OrderPathStoreError>;
    fn load_by_request_id(&self, request_id: StrategyRequestId) -> Option<OrderPathRecord>;
    fn load_by_client_order_id(&self, client_order_id: &ClientOrderId) -> Option<OrderPathRecord>;
    fn load_by_broker_order_id(&self, broker_order_id: &BrokerOrderId) -> Option<OrderPathRecord>;
    fn update_record(&mut self, record: OrderPathRecord) -> Result<(), OrderPathStoreError>;
    fn all_records(&self) -> Vec<OrderPathRecord>;
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryOrderPathStore {
    by_request_id: HashMap<StrategyRequestId, OrderPathRecord>,
    request_by_client_id: HashMap<ClientOrderId, StrategyRequestId>,
    request_by_broker_id: HashMap<BrokerOrderId, StrategyRequestId>,
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
        if let Some(broker_order_id) = record.broker_order_id.as_ref() {
            if self.request_by_broker_id.contains_key(broker_order_id) {
                return Err(OrderPathStoreError::DuplicateBrokerOrderId(
                    broker_order_id.clone(),
                ));
            }
        }
        self.request_by_client_id
            .insert(record.client_order_id.clone(), record.request_id);
        if let Some(broker_order_id) = record.broker_order_id.as_ref() {
            self.request_by_broker_id
                .insert(broker_order_id.clone(), record.request_id);
        }
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

    pub fn get_by_broker_order_id(
        &self,
        broker_order_id: &BrokerOrderId,
    ) -> Option<&OrderPathRecord> {
        self.request_by_broker_id
            .get(broker_order_id)
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
        match (&existing.broker_order_id, &record.broker_order_id) {
            (Some(existing_id), Some(new_id)) if existing_id != new_id => {
                return Err(OrderPathStoreError::BrokerOrderIdChanged(record.request_id));
            }
            (Some(_), None) => {
                return Err(OrderPathStoreError::BrokerOrderIdChanged(record.request_id));
            }
            _ => {}
        }
        if existing.broker_order_id.is_none() {
            if let Some(new_broker_order_id) = record.broker_order_id.as_ref() {
                if let Some(mapped_request_id) = self.request_by_broker_id.get(new_broker_order_id)
                {
                    if mapped_request_id != &record.request_id {
                        return Err(OrderPathStoreError::DuplicateBrokerOrderId(
                            new_broker_order_id.clone(),
                        ));
                    }
                }
            }
        }
        if is_terminal_order_path_state(existing.state)
            && !is_terminal_order_path_state(record.state)
        {
            return Err(OrderPathStoreError::TerminalStateOverwrite(
                record.request_id,
            ));
        }
        if record.last_update_ts < existing.last_update_ts {
            return Err(OrderPathStoreError::UpdatedTimestampRegressed(
                record.request_id,
            ));
        }
        if existing.broker_order_id.is_none() {
            if let Some(new_broker_order_id) = record.broker_order_id.as_ref() {
                self.request_by_broker_id
                    .insert(new_broker_order_id.clone(), record.request_id);
            }
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

    fn load_by_broker_order_id(&self, broker_order_id: &BrokerOrderId) -> Option<OrderPathRecord> {
        self.get_by_broker_order_id(broker_order_id).cloned()
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

    fn load_by_broker_order_id(&self, broker_order_id: &BrokerOrderId) -> Option<OrderPathRecord> {
        self.inner.load_by_broker_order_id(broker_order_id)
    }

    fn update_record(&mut self, record: OrderPathRecord) -> Result<(), OrderPathStoreError> {
        self.inner.update_record(record)?;
        self.persist()
    }

    fn all_records(&self) -> Vec<OrderPathRecord> {
        self.inner.all_records()
    }
}

pub struct SqliteOrderPathStore {
    path: PathBuf,
    lock_path: PathBuf,
    _writer_lock: File,
    writer_lock_metadata: SqliteWriterLockMetadata,
    connection: Connection,
    inner: InMemoryOrderPathStore,
}

pub struct SqliteOrderPathReadStore {
    connection: Connection,
    inner: InMemoryOrderPathStore,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqliteWriterLockMetadata {
    pub instance_id: String,
    pub pid: u32,
    pub created_ts: DateTime<Utc>,
    pub schema_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqliteOrderPathRedactedRecord {
    pub request_id: StrategyRequestId,
    pub client_order_id_fingerprint: RedactedValueFingerprint,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub broker_order_id_fingerprint: Option<RedactedValueFingerprint>,
    pub account_id_fingerprint: RedactedValueFingerprint,
    pub state: OrderPathState,
    pub last_update_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqliteOrderPathTransitionAudit {
    pub id: i64,
    pub request_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_state: Option<String>,
    pub to_state: String,
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    pub ts: String,
    pub safe_details: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SqliteRuntimeDirectoryIssue {
    Missing,
    NotDirectory,
    GroupOrWorldAccessible,
    InsideWorkspaceTree,
    InsideWorkspaceArtifactArea,
}

pub fn inspect_sqlite_runtime_directory(
    runtime_dir: &Path,
    workspace_root: Option<&Path>,
) -> Vec<SqliteRuntimeDirectoryIssue> {
    let mut issues = Vec::new();
    match fs::metadata(runtime_dir) {
        Ok(metadata) if !metadata.is_dir() => {
            issues.push(SqliteRuntimeDirectoryIssue::NotDirectory)
        }
        Ok(metadata) => {
            if sqlite_runtime_directory_is_group_or_world_accessible(&metadata) {
                issues.push(SqliteRuntimeDirectoryIssue::GroupOrWorldAccessible);
            }
        }
        Err(_) => issues.push(SqliteRuntimeDirectoryIssue::Missing),
    }

    if let Some(workspace_root) = workspace_root {
        if runtime_dir.starts_with(workspace_root) {
            issues.push(SqliteRuntimeDirectoryIssue::InsideWorkspaceTree);
        }
        if runtime_dir.starts_with(workspace_root.join("reports"))
            || runtime_dir.starts_with(workspace_root.join("tmp"))
            || runtime_dir.starts_with(workspace_root.join("handoff"))
        {
            issues.push(SqliteRuntimeDirectoryIssue::InsideWorkspaceArtifactArea);
        }
    }

    issues
}

impl SqliteOrderPathStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, OrderPathStoreError> {
        let path = path.into();
        if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
            fs::create_dir_all(parent)
                .map_err(|error| OrderPathStoreError::Io(error.to_string()))?;
        }
        let lock_path = sqlite_writer_lock_path(&path);
        let (writer_lock, writer_lock_metadata) = create_sqlite_writer_lock(&lock_path)?;
        let connection = match Connection::open(&path) {
            Ok(connection) => connection,
            Err(error) => {
                let _ = fs::remove_file(&lock_path);
                return Err(sqlite_store_error(error));
            }
        };
        if let Err(error) = harden_sqlite_runtime_file_permissions(&path, Some(&lock_path)) {
            let _ = fs::remove_file(&lock_path);
            return Err(error);
        }
        let mut store = Self {
            path,
            lock_path,
            _writer_lock: writer_lock,
            writer_lock_metadata,
            connection,
            inner: InMemoryOrderPathStore::default(),
        };
        if let Err(error) = store.initialize_and_load() {
            let _ = fs::remove_file(&store.lock_path);
            return Err(error);
        }
        if let Err(error) =
            harden_sqlite_runtime_file_permissions(&store.path, Some(&store.lock_path))
        {
            let _ = fs::remove_file(&store.lock_path);
            return Err(error);
        }
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }

    pub fn writer_lock_metadata(&self) -> &SqliteWriterLockMetadata {
        &self.writer_lock_metadata
    }

    pub fn redacted_records(&self) -> Vec<SqliteOrderPathRedactedRecord> {
        redacted_records_from_inner(&self.inner)
    }

    pub fn transition_audit(
        &self,
    ) -> Result<Vec<SqliteOrderPathTransitionAudit>, OrderPathStoreError> {
        load_transition_audit_from_sqlite(&self.connection)
    }

    fn initialize_and_load(&mut self) -> Result<(), OrderPathStoreError> {
        self.connection
            .execute_batch(
                r#"
                PRAGMA journal_mode = WAL;
                PRAGMA synchronous = FULL;
                PRAGMA foreign_keys = ON;
                PRAGMA busy_timeout = 5000;
                CREATE TABLE IF NOT EXISTS order_path_schema (
                    key TEXT PRIMARY KEY,
                    value INTEGER NOT NULL
                );
                "#,
            )
            .map_err(sqlite_store_error)?;
        ensure_sqlite_schema_version(&self.connection)?;
        self.connection
            .execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS order_path_records (
                    request_id TEXT PRIMARY KEY,
                    client_order_id TEXT NOT NULL UNIQUE,
                    broker_order_id TEXT UNIQUE,
                    state TEXT NOT NULL,
                    last_update_ts TEXT NOT NULL,
                    payload TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS order_path_transitions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    request_id TEXT NOT NULL,
                    from_state TEXT,
                    to_state TEXT NOT NULL,
                    event TEXT NOT NULL,
                    reason_code TEXT,
                    ts TEXT NOT NULL,
                    safe_details TEXT NOT NULL
                );
                "#,
            )
            .map_err(sqlite_store_error)?;
        self.inner = load_inner_from_sqlite(&self.connection)?;
        Ok(())
    }

    fn insert_record_sql(&mut self, record: &OrderPathRecord) -> Result<(), OrderPathStoreError> {
        let payload = serde_json::to_string(record)
            .map_err(|error| OrderPathStoreError::Serialization(error.to_string()))?;
        let broker_order_id = record.broker_order_id.as_ref().map(|value| value.as_str());
        let state = format!("{:?}", record.state);
        let last_update_ts = record.last_update_ts.to_rfc3339();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sqlite_store_error)?;
        transaction
            .execute(
                "INSERT INTO order_path_records \
                 (request_id, client_order_id, broker_order_id, state, last_update_ts, payload) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    record.request_id.to_string(),
                    record.client_order_id.as_str(),
                    broker_order_id,
                    state,
                    last_update_ts,
                    payload
                ],
            )
            .map_err(sqlite_store_error)?;
        insert_transition_audit_sql(
            &transaction,
            record.request_id,
            None,
            record.state,
            "InsertIntent",
            record_reason_code(record),
            record.last_update_ts,
        )?;
        transaction.commit().map_err(sqlite_store_error)?;
        harden_sqlite_runtime_file_permissions(&self.path, Some(&self.lock_path))?;
        Ok(())
    }

    fn update_record_sql(
        &mut self,
        previous: &OrderPathRecord,
        record: &OrderPathRecord,
    ) -> Result<(), OrderPathStoreError> {
        let payload = serde_json::to_string(record)
            .map_err(|error| OrderPathStoreError::Serialization(error.to_string()))?;
        let broker_order_id = record.broker_order_id.as_ref().map(|value| value.as_str());
        let state = format!("{:?}", record.state);
        let last_update_ts = record.last_update_ts.to_rfc3339();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sqlite_store_error)?;
        let affected = transaction
            .execute(
                "UPDATE order_path_records \
                 SET client_order_id = ?2, broker_order_id = ?3, state = ?4, \
                     last_update_ts = ?5, payload = ?6 \
                 WHERE request_id = ?1",
                params![
                    record.request_id.to_string(),
                    record.client_order_id.as_str(),
                    broker_order_id,
                    state,
                    last_update_ts,
                    payload
                ],
            )
            .map_err(sqlite_store_error)?;
        if affected != 1 {
            return Err(OrderPathStoreError::RecordNotFound(record.request_id));
        }
        insert_transition_audit_sql(
            &transaction,
            record.request_id,
            Some(previous.state),
            record.state,
            infer_transition_audit_event(previous, record),
            record_reason_code(record),
            record.last_update_ts,
        )?;
        transaction.commit().map_err(sqlite_store_error)?;
        harden_sqlite_runtime_file_permissions(&self.path, Some(&self.lock_path))?;
        Ok(())
    }
}

/// Operator/internal diagnostic store.
///
/// This surface is read-only/query-only at SQLite level, but methods prefixed
/// with `operator_` return raw local `OrderPathRecord` payloads and therefore
/// must not be used for review exports or runtime-facing reporting. Use
/// `redacted_records()` for review/export surfaces.
impl SqliteOrderPathReadStore {
    pub fn open_readonly(path: impl Into<PathBuf>) -> Result<Self, OrderPathStoreError> {
        let path = path.into();
        let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(sqlite_store_error)?;
        connection
            .execute_batch(
                r#"
                PRAGMA query_only = ON;
                PRAGMA foreign_keys = ON;
                PRAGMA busy_timeout = 5000;
                "#,
            )
            .map_err(sqlite_store_error)?;
        ensure_sqlite_schema_version(&connection)?;
        let inner = load_inner_from_sqlite(&connection)?;
        Ok(Self { connection, inner })
    }

    pub fn operator_load_by_request_id(
        &self,
        request_id: StrategyRequestId,
    ) -> Option<OrderPathRecord> {
        self.inner.load_by_request_id(request_id)
    }

    pub fn operator_load_by_client_order_id(
        &self,
        client_order_id: &ClientOrderId,
    ) -> Option<OrderPathRecord> {
        self.inner.load_by_client_order_id(client_order_id)
    }

    pub fn operator_load_by_broker_order_id(
        &self,
        broker_order_id: &BrokerOrderId,
    ) -> Option<OrderPathRecord> {
        self.inner.load_by_broker_order_id(broker_order_id)
    }

    pub fn operator_all_records(&self) -> Vec<OrderPathRecord> {
        self.inner.all_records()
    }

    pub fn redacted_records(&self) -> Vec<SqliteOrderPathRedactedRecord> {
        redacted_records_from_inner(&self.inner)
    }

    pub fn transition_audit(
        &self,
    ) -> Result<Vec<SqliteOrderPathTransitionAudit>, OrderPathStoreError> {
        load_transition_audit_from_sqlite(&self.connection)
    }
}

impl Drop for SqliteOrderPathStore {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock_path);
    }
}

impl OrderPathStore for SqliteOrderPathStore {
    fn insert_intent(&mut self, record: OrderPathRecord) -> Result<(), OrderPathStoreError> {
        let mut next_inner = self.inner.clone();
        next_inner.insert_intent(record.clone())?;
        self.insert_record_sql(&record)?;
        self.inner = next_inner;
        Ok(())
    }

    fn load_by_request_id(&self, request_id: StrategyRequestId) -> Option<OrderPathRecord> {
        self.inner.load_by_request_id(request_id)
    }

    fn load_by_client_order_id(&self, client_order_id: &ClientOrderId) -> Option<OrderPathRecord> {
        self.inner.load_by_client_order_id(client_order_id)
    }

    fn load_by_broker_order_id(&self, broker_order_id: &BrokerOrderId) -> Option<OrderPathRecord> {
        self.inner.load_by_broker_order_id(broker_order_id)
    }

    fn update_record(&mut self, record: OrderPathRecord) -> Result<(), OrderPathStoreError> {
        let mut next_inner = self.inner.clone();
        let previous = self
            .inner
            .load_by_request_id(record.request_id)
            .ok_or(OrderPathStoreError::RecordNotFound(record.request_id))?;
        next_inner.update_record(record.clone())?;
        self.update_record_sql(&previous, &record)?;
        self.inner = next_inner;
        Ok(())
    }

    fn all_records(&self) -> Vec<OrderPathRecord> {
        self.inner.all_records()
    }
}

fn sqlite_writer_lock_path(path: &Path) -> PathBuf {
    let mut lock_path = path.to_path_buf();
    lock_path.set_extension("writer.lock");
    lock_path
}

fn sqlite_wal_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}-wal", path.to_string_lossy()))
}

fn sqlite_shm_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}-shm", path.to_string_lossy()))
}

fn create_sqlite_writer_lock(
    lock_path: &Path,
) -> Result<(File, SqliteWriterLockMetadata), OrderPathStoreError> {
    let metadata = SqliteWriterLockMetadata {
        instance_id: format!(
            "sqlite-writer-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ),
        pid: std::process::id(),
        created_ts: Utc::now(),
        schema_version: SQLITE_ORDER_PATH_SCHEMA_VERSION,
    };
    let mut writer_lock = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(lock_path)
        .map_err(|error| sqlite_writer_lock_unavailable(lock_path, error))?;
    let raw = serde_json::to_string_pretty(&metadata)
        .map_err(|error| OrderPathStoreError::Serialization(error.to_string()))?;
    if let Err(error) = writer_lock.write_all(raw.as_bytes()) {
        let _ = fs::remove_file(lock_path);
        return Err(OrderPathStoreError::Io(error.to_string()));
    }
    if let Err(error) = writer_lock.sync_all() {
        let _ = fs::remove_file(lock_path);
        return Err(OrderPathStoreError::Io(error.to_string()));
    }
    if let Err(error) = harden_existing_sqlite_runtime_file(lock_path) {
        let _ = fs::remove_file(lock_path);
        return Err(error);
    }
    Ok((writer_lock, metadata))
}

fn sqlite_writer_lock_unavailable(path: &Path, error: std::io::Error) -> OrderPathStoreError {
    let summary = fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<SqliteWriterLockMetadata>(&raw).ok())
        .map(|metadata| {
            format!(
                "writer lock present: pid={} created_ts={} instance_id_len={} schema_version={}",
                metadata.pid,
                metadata.created_ts,
                metadata.instance_id.len(),
                metadata.schema_version
            )
        })
        .unwrap_or_else(|| format!("writer lock unavailable: {error}"));
    OrderPathStoreError::WriterLockUnavailable(summary)
}

fn ensure_sqlite_schema_version(connection: &Connection) -> Result<(), OrderPathStoreError> {
    let version = connection
        .query_row(
            "SELECT value FROM order_path_schema WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(sqlite_store_error)?;
    match version {
        Some(value) if value == SQLITE_ORDER_PATH_SCHEMA_VERSION => Ok(()),
        Some(value) => Err(OrderPathStoreError::SchemaVersionMismatch {
            expected: SQLITE_ORDER_PATH_SCHEMA_VERSION,
            found: value,
        }),
        None => {
            connection
                .execute(
                    "INSERT INTO order_path_schema (key, value) VALUES ('schema_version', ?1)",
                    params![SQLITE_ORDER_PATH_SCHEMA_VERSION],
                )
                .map_err(sqlite_store_error)?;
            Ok(())
        }
    }
}

fn load_inner_from_sqlite(
    connection: &Connection,
) -> Result<InMemoryOrderPathStore, OrderPathStoreError> {
    let mut statement = connection
        .prepare("SELECT payload FROM order_path_records ORDER BY request_id")
        .map_err(sqlite_store_error)?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(sqlite_store_error)?;
    let mut inner = InMemoryOrderPathStore::default();
    for row in rows {
        let payload = row.map_err(sqlite_store_error)?;
        let record: OrderPathRecord = serde_json::from_str(&payload)
            .map_err(|error| OrderPathStoreError::Serialization(error.to_string()))?;
        inner.insert_intent(record)?;
    }
    Ok(inner)
}

fn redacted_records_from_inner(
    inner: &InMemoryOrderPathStore,
) -> Vec<SqliteOrderPathRedactedRecord> {
    let mut records: Vec<_> = inner
        .all_records()
        .into_iter()
        .map(|record| SqliteOrderPathRedactedRecord {
            request_id: record.request_id,
            client_order_id_fingerprint: fingerprint_redacted_value(
                record.client_order_id.as_str(),
            ),
            broker_order_id_fingerprint: record
                .broker_order_id
                .as_ref()
                .map(|broker_order_id| fingerprint_redacted_value(broker_order_id.as_str())),
            account_id_fingerprint: fingerprint_redacted_value(record.account_id.as_str()),
            state: record.state,
            last_update_ts: record.last_update_ts,
        })
        .collect();
    records.sort_by(|left, right| {
        left.request_id
            .to_string()
            .cmp(&right.request_id.to_string())
    });
    records
}

fn insert_transition_audit_sql(
    transaction: &rusqlite::Transaction<'_>,
    request_id: StrategyRequestId,
    from_state: Option<OrderPathState>,
    to_state: OrderPathState,
    event: &'static str,
    reason_code: Option<String>,
    ts: DateTime<Utc>,
) -> Result<(), OrderPathStoreError> {
    transaction
        .execute(
            "INSERT INTO order_path_transitions \
             (request_id, from_state, to_state, event, reason_code, ts, safe_details) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                request_id.to_string(),
                from_state.map(|state| format!("{state:?}")),
                format!("{to_state:?}"),
                event,
                reason_code,
                ts.to_rfc3339(),
                "sqlite_order_path_store"
            ],
        )
        .map_err(sqlite_store_error)?;
    Ok(())
}

fn load_transition_audit_from_sqlite(
    connection: &Connection,
) -> Result<Vec<SqliteOrderPathTransitionAudit>, OrderPathStoreError> {
    let mut statement = connection
        .prepare(
            "SELECT id, request_id, from_state, to_state, event, reason_code, ts, safe_details \
             FROM order_path_transitions ORDER BY id",
        )
        .map_err(sqlite_store_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok(SqliteOrderPathTransitionAudit {
                id: row.get(0)?,
                request_id: row.get(1)?,
                from_state: row.get(2)?,
                to_state: row.get(3)?,
                event: row.get(4)?,
                reason_code: row.get(5)?,
                ts: row.get(6)?,
                safe_details: row.get(7)?,
            })
        })
        .map_err(sqlite_store_error)?;
    let mut entries = Vec::new();
    for row in rows {
        entries.push(row.map_err(sqlite_store_error)?);
    }
    Ok(entries)
}

fn record_reason_code(record: &OrderPathRecord) -> Option<String> {
    record
        .last_error_kind
        .map(|kind| format!("{kind:?}"))
        .or_else(|| record.last_ack_status.map(|status| format!("{status:?}")))
}

fn infer_transition_audit_event(
    previous: &OrderPathRecord,
    record: &OrderPathRecord,
) -> &'static str {
    use OrderPathState as S;

    match (previous.state, record.state) {
        (S::IntentRecorded, S::LocalRejected) => "LocalReject",
        (S::IntentRecorded, S::SubmitInFlight) => "BeginSubmit",
        (S::SubmitInFlight, S::Submitted) => "SubmitAccepted",
        (S::SubmitInFlight, S::SubmittedPendingBrokerOrderId) => {
            "SubmitAcceptedWithoutBrokerOrderId"
        }
        (S::SubmitInFlight, S::TimeoutUnknownPending) => "SubmitTimedOut",
        (S::TimeoutUnknownPending, S::RecoveredByClientOrderId)
        | (S::SubmittedPendingBrokerOrderId, S::RecoveredByClientOrderId) => {
            "RecoverByClientOrderId"
        }
        (S::SubmitInFlight, S::ManualInterventionRequired)
        | (S::SubmittedPendingBrokerOrderId, S::ManualInterventionRequired)
        | (S::TimeoutUnknownPending, S::ManualInterventionRequired)
        | (S::CancelTimeoutUnknownPending, S::ManualInterventionRequired) => {
            "RequireManualIntervention"
        }
        (S::SubmitInFlight, S::BrokerRejected) | (S::Submitted, S::BrokerRejected) => {
            "BrokerReject"
        }
        (S::Submitted, S::CancelRequested) | (S::RecoveredByClientOrderId, S::CancelRequested) => {
            "RequestCancel"
        }
        (S::CancelRequested, S::CancelSubmitted) => "CancelAccepted",
        (S::CancelRequested, S::ManualInterventionRequired)
        | (S::CancelSubmitted, S::ManualInterventionRequired)
            if record.last_error_kind == Some(OrderPathErrorKind::BrokerRejected) =>
        {
            "CancelRejected"
        }
        (S::CancelRequested, S::ManualInterventionRequired)
        | (S::CancelSubmitted, S::ManualInterventionRequired) => "RequireManualIntervention",
        (S::CancelRequested, S::CancelTimeoutUnknownPending)
        | (S::CancelSubmitted, S::CancelTimeoutUnknownPending) => "CancelTimedOut",
        (S::CancelTimeoutUnknownPending, S::CancelRecoveredTerminal) => "RecoverCancelTerminal",
        (_, S::Terminal) => "MarkTerminal",
        _ => "UpdateRecord",
    }
}

#[cfg(unix)]
fn sqlite_runtime_directory_is_group_or_world_accessible(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;

    metadata.permissions().mode() & 0o077 != 0
}

#[cfg(not(unix))]
fn sqlite_runtime_directory_is_group_or_world_accessible(_metadata: &fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn harden_existing_sqlite_runtime_file(path: &Path) -> Result<(), OrderPathStoreError> {
    use std::os::unix::fs::PermissionsExt;

    let permissions = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, permissions)
        .map_err(|error| OrderPathStoreError::Io(error.to_string()))
}

#[cfg(not(unix))]
fn harden_existing_sqlite_runtime_file(_path: &Path) -> Result<(), OrderPathStoreError> {
    Ok(())
}

fn harden_sqlite_runtime_file_permissions(
    db_path: &Path,
    lock_path: Option<&Path>,
) -> Result<(), OrderPathStoreError> {
    let wal_path = sqlite_wal_path(db_path);
    let shm_path = sqlite_shm_path(db_path);
    for path in [
        Some(db_path),
        Some(wal_path.as_path()),
        Some(shm_path.as_path()),
        lock_path,
    ]
    .into_iter()
    .flatten()
    {
        if path.exists() {
            harden_existing_sqlite_runtime_file(path)?;
        }
    }
    Ok(())
}

fn sqlite_store_error(error: rusqlite::Error) -> OrderPathStoreError {
    OrderPathStoreError::Sqlite(error.to_string())
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
        if self.one_shot && self.endpoint_attempted {
            return Err(OrderPreflightError::OneShotAlreadyUsed);
        }
        if !self.endpoint_calls_enabled {
            return Err(OrderPreflightError::EndpointNotArmed);
        }
        if now >= self.armed_until {
            return Err(OrderPreflightError::ArmExpired);
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

    pub fn disarm_for_safety_signal(
        &mut self,
        signal: OperatorDisarmSignal,
    ) -> OperatorDisarmDecision {
        let was_enabled = self.endpoint_calls_enabled;
        self.endpoint_calls_enabled = false;
        OperatorDisarmDecision {
            signal,
            was_enabled,
            endpoint_calls_enabled: self.endpoint_calls_enabled,
        }
    }

    pub fn rearm(
        &mut self,
        session_id: impl Into<String>,
        armed_until: DateTime<Utc>,
        preflight_digest: impl Into<String>,
        one_shot: bool,
    ) {
        self.session_id = session_id.into();
        self.armed_until = armed_until;
        self.preflight_digest = preflight_digest.into();
        self.one_shot = one_shot;
        self.endpoint_attempted = false;
        self.endpoint_calls_enabled = true;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OperatorDisarmSignal {
    GatewayDegraded,
    OrderPathStoreLockUncertain,
    OrderPathStoreMigrationMismatch,
    OrderPathStoreUnavailable,
    RuntimeBridgeDeadLetter,
    UnknownPendingOrder,
    AcceptedWithoutBrokerOrderId,
    CancelBrokerOrderIdMismatch,
    CancelTimeoutUnknownPending,
    OrderEndpointRateLimited,
    OrderEndpointMaintenance,
    OrderEndpointDecodeError,
    OrderEndpointUnauthorized,
    ReconciliationStale,
    RestartRecovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperatorDisarmDecision {
    pub signal: OperatorDisarmSignal,
    pub was_enabled: bool,
    pub endpoint_calls_enabled: bool,
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

    pub fn approve_place_order(
        &self,
        order: &PlaceOrder,
        now: DateTime<Utc>,
    ) -> Result<PreflightApprovedPlaceOrder, OrderPreflightError> {
        self.approve_place_order_with_context(order, now, &OrderPreflightContext::default())
    }

    pub fn validate_place_order_with_context(
        &self,
        order: &PlaceOrder,
        now: DateTime<Utc>,
        context: &OrderPreflightContext,
    ) -> Result<(), OrderPreflightError> {
        self.operator_arm.validate(now)?;
        validate_command_ttl(order.created_ts, order.ttl_ms, now)?;
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
        if order.comment.is_some() {
            return Err(OrderPreflightError::RawCommandCommentNotAllowed);
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

    pub fn approve_place_order_with_context(
        &self,
        order: &PlaceOrder,
        now: DateTime<Utc>,
        context: &OrderPreflightContext,
    ) -> Result<PreflightApprovedPlaceOrder, OrderPreflightError> {
        self.validate_place_order_with_context(order, now, context)?;
        Ok(PreflightApprovedPlaceOrder {
            order: order.clone(),
            approved_ts: now,
        })
    }

    pub fn validate_cancel_order(
        &self,
        cancel: &CancelOrder,
        now: DateTime<Utc>,
        existing: Option<&OrderPathRecord>,
    ) -> Result<CancelPreflightDecision, OrderPreflightError> {
        self.operator_arm.validate(now)?;
        validate_command_ttl(cancel.created_ts, cancel.ttl_ms, now)?;
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
            let mapped_order_id = record
                .broker_order_id
                .as_ref()
                .ok_or(OrderPreflightError::CancelMappingMissing)?;
            if mapped_order_id != &cancel.order_id {
                return Err(OrderPreflightError::CancelMappingMismatch);
            }
            if matches!(
                record.state,
                OrderPathState::SubmittedPendingBrokerOrderId
                    | OrderPathState::TimeoutUnknownPending
                    | OrderPathState::CancelTimeoutUnknownPending
            ) {
                return Err(OrderPreflightError::CancelStateRequiresManualIntervention);
            }
            if matches!(
                record.state,
                OrderPathState::CancelRequested | OrderPathState::CancelSubmitted
            ) {
                return Err(OrderPreflightError::CancelAlreadyPending);
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

    pub fn approve_cancel_order(
        &self,
        cancel: &CancelOrder,
        now: DateTime<Utc>,
        existing: Option<&OrderPathRecord>,
    ) -> Result<CancelPreflightApproval, OrderPreflightError> {
        match self.validate_cancel_order(cancel, now, existing)? {
            CancelPreflightDecision::SubmitCancel => Ok(CancelPreflightApproval::Submit(
                PreflightApprovedCancelOrder {
                    cancel: cancel.clone(),
                    approved_ts: now,
                    mapped_request_id: existing.map(|record| record.request_id),
                },
            )),
            CancelPreflightDecision::AlreadyTerminal => {
                Ok(CancelPreflightApproval::AlreadyTerminal)
            }
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

#[derive(Debug, Clone, PartialEq)]
pub struct PreflightApprovedPlaceOrder {
    order: PlaceOrder,
    approved_ts: DateTime<Utc>,
}

impl PreflightApprovedPlaceOrder {
    pub fn order(&self) -> &PlaceOrder {
        &self.order
    }

    pub fn approved_ts(&self) -> DateTime<Utc> {
        self.approved_ts
    }

    pub fn into_inner(self) -> PlaceOrder {
        self.order
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreflightApprovedCancelOrder {
    cancel: CancelOrder,
    approved_ts: DateTime<Utc>,
    mapped_request_id: Option<StrategyRequestId>,
}

impl PreflightApprovedCancelOrder {
    pub fn cancel(&self) -> &CancelOrder {
        &self.cancel
    }

    pub fn approved_ts(&self) -> DateTime<Utc> {
        self.approved_ts
    }

    pub fn mapped_request_id(&self) -> Option<StrategyRequestId> {
        self.mapped_request_id
    }

    pub fn into_inner(self) -> CancelOrder {
        self.cancel
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CancelPreflightApproval {
    Submit(PreflightApprovedCancelOrder),
    AlreadyTerminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DryOrderRateLimit {
    pub capacity: u32,
    pub used: u32,
}

impl DryOrderRateLimit {
    pub fn new(capacity: u32) -> Self {
        Self { capacity, used: 0 }
    }

    pub fn remaining(&self) -> u32 {
        self.capacity.saturating_sub(self.used)
    }

    pub fn try_consume(&mut self, permits: u32) -> Result<(), DryOrderRateLimitError> {
        if permits == 0 {
            return Ok(());
        }
        if self.remaining() < permits {
            return Err(DryOrderRateLimitError::CapacityExhausted);
        }
        self.used += permits;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DryOrderRateLimitError {
    #[error("dry order rate limit capacity exhausted")]
    CapacityExhausted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DryOrderRateWindow {
    pub capacity: u32,
    pub used: u32,
    pub window_started_ts: DateTime<Utc>,
    pub window_ms: u64,
    pub backoff_until: Option<DateTime<Utc>>,
}

impl DryOrderRateWindow {
    pub fn new(capacity: u32, window_ms: u64, now: DateTime<Utc>) -> Self {
        Self {
            capacity,
            used: 0,
            window_started_ts: now,
            window_ms: window_ms.max(1),
            backoff_until: None,
        }
    }

    pub fn with_backoff_until(mut self, backoff_until: DateTime<Utc>) -> Self {
        self.backoff_until = Some(backoff_until);
        self
    }

    pub fn remaining(&self, now: DateTime<Utc>) -> u32 {
        if self.window_elapsed(now) {
            self.capacity
        } else {
            self.capacity.saturating_sub(self.used)
        }
    }

    pub fn try_consume(
        &mut self,
        now: DateTime<Utc>,
        permits: u32,
    ) -> Result<DryOrderRateWindowDecision, DryOrderRateWindowError> {
        if let Some(backoff_until) = self.backoff_until {
            if now < backoff_until {
                return Err(DryOrderRateWindowError::BackoffActive {
                    retry_after_ms: duration_ms_floor(backoff_until.signed_duration_since(now)),
                });
            }
            self.backoff_until = None;
        }
        if self.window_elapsed(now) {
            self.used = 0;
            self.window_started_ts = now;
        }
        if permits == 0 {
            return Ok(DryOrderRateWindowDecision {
                remaining: self.remaining(now),
            });
        }
        if self.remaining(now) < permits {
            let reset_after = chrono::Duration::milliseconds(self.window_ms as i64)
                - now.signed_duration_since(self.window_started_ts);
            return Err(DryOrderRateWindowError::CapacityExhausted {
                reset_after_ms: duration_ms_floor(reset_after),
            });
        }
        self.used += permits;
        Ok(DryOrderRateWindowDecision {
            remaining: self.remaining(now),
        })
    }

    fn window_elapsed(&self, now: DateTime<Utc>) -> bool {
        now.signed_duration_since(self.window_started_ts)
            .num_milliseconds()
            >= self.window_ms as i64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DryOrderRateWindowDecision {
    pub remaining: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DryOrderRateWindowError {
    #[error("dry order rate window capacity exhausted; reset after {reset_after_ms} ms")]
    CapacityExhausted { reset_after_ms: u64 },
    #[error("dry order rate backoff active; retry after {retry_after_ms} ms")]
    BackoffActive { retry_after_ms: u64 },
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
    #[error("command ttl expired")]
    CommandExpired,
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
    #[error("raw broker command comment is not allowed")]
    RawCommandCommentNotAllowed,
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
    #[error("cancel mapping does not match requested broker order id")]
    CancelMappingMismatch,
    #[error("cancel is already pending for this broker order id")]
    CancelAlreadyPending,
    #[error("cancel state requires reconciliation or manual intervention")]
    CancelStateRequiresManualIntervention,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OrderPathReconciliationError {
    #[error("order-path record not found by client_order_id: {0}")]
    RecordNotFoundByClientOrderId(ClientOrderId),
    #[error("order-path record already has broker_order_id: {0}")]
    BrokerOrderIdAlreadySet(StrategyRequestId),
    #[error("order-path broker_order_id mismatch for request: {0}")]
    BrokerOrderIdMismatch(StrategyRequestId),
    #[error("order-path state is not client-id-recoverable for request {request_id}: {state:?}")]
    StateNotRecoverableByClientOrderId {
        request_id: StrategyRequestId,
        state: OrderPathState,
    },
    #[error("order-path store error during reconciliation: {0}")]
    Store(#[from] OrderPathStoreError),
    #[error("order-path transition error during reconciliation: {0}")]
    Transition(#[from] OrderPathTransitionError),
}

pub fn recover_order_path_by_client_order_id<S>(
    store: &mut S,
    client_order_id: &ClientOrderId,
    broker_order_id: BrokerOrderId,
    now: DateTime<Utc>,
) -> Result<OrderPathRecord, OrderPathReconciliationError>
where
    S: OrderPathStore,
{
    let mut record = store
        .load_by_client_order_id(client_order_id)
        .ok_or_else(|| {
            OrderPathReconciliationError::RecordNotFoundByClientOrderId(client_order_id.clone())
        })?;
    if let Some(existing_broker_order_id) = record.broker_order_id.as_ref() {
        if existing_broker_order_id == &broker_order_id {
            return Ok(record);
        }
        return Err(OrderPathReconciliationError::BrokerOrderIdMismatch(
            record.request_id,
        ));
    }
    if !matches!(
        record.state,
        OrderPathState::SubmittedPendingBrokerOrderId | OrderPathState::TimeoutUnknownPending
    ) {
        return Err(
            OrderPathReconciliationError::StateNotRecoverableByClientOrderId {
                request_id: record.request_id,
                state: record.state,
            },
        );
    }

    record.broker_order_id = Some(broker_order_id);
    record.transition(OrderPathEvent::RecoverByClientOrderId, now)?;
    store.update_record(record.clone())?;
    Ok(record)
}

fn validate_command_ttl(
    created_ts: DateTime<Utc>,
    ttl_ms: Option<u64>,
    now: DateTime<Utc>,
) -> Result<(), OrderPreflightError> {
    let Some(ttl_ms) = ttl_ms else {
        return Ok(());
    };
    let age_ms = now.signed_duration_since(created_ts).num_milliseconds();
    if age_ms > ttl_ms as i64 {
        Err(OrderPreflightError::CommandExpired)
    } else {
        Ok(())
    }
}

fn duration_ms_floor(duration: chrono::Duration) -> u64 {
    duration.num_milliseconds().max(0) as u64
}

fn is_decimal_multiple(value: Decimal, step: Decimal) -> bool {
    step > Decimal::ZERO && value % step == Decimal::ZERO
}

fn is_terminal_order_path_state(state: OrderPathState) -> bool {
    matches!(
        state,
        OrderPathState::Terminal
            | OrderPathState::CancelRecoveredTerminal
            | OrderPathState::LocalRejected
            | OrderPathState::BrokerRejected
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::AccountId;
    use crate::command::{CancelOrder, CommandAckReason, PlaceOrder};
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

    fn temp_runtime_dir(name: &str) -> std::path::PathBuf {
        let unique = Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_default()
            .unsigned_abs();
        std::env::temp_dir().join(format!("moex_trading_order_path_{name}_{unique}"))
    }

    fn cleanup_sqlite_store(path: &Path) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(sqlite_writer_lock_path(path));
        let _ = std::fs::remove_file(sqlite_wal_path(path));
        let _ = std::fs::remove_file(sqlite_shm_path(path));
    }

    #[cfg(unix)]
    fn assert_sqlite_runtime_file_is_protected(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let mode = std::fs::metadata(path)
            .expect("sqlite runtime file metadata")
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o077,
            0,
            "{path:?} must not be group/world accessible"
        );
    }

    #[cfg(unix)]
    fn assert_existing_sqlite_runtime_files_are_protected(path: &Path) {
        for runtime_path in [
            path.to_path_buf(),
            sqlite_wal_path(path),
            sqlite_shm_path(path),
            sqlite_writer_lock_path(path),
        ] {
            if runtime_path.exists() {
                assert_sqlite_runtime_file_is_protected(&runtime_path);
            }
        }
    }

    #[cfg(unix)]
    fn chmod(path: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;

        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .expect("set permissions");
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
    fn order_path_accepted_without_broker_id_requires_reconciliation_before_cancel() {
        let now = Utc::now();
        let order = place_order(request_id(49), "CID000000000000049");
        let mut record = OrderPathRecord::from_place_order(&order, now, None);

        record
            .transition(OrderPathEvent::BeginSubmit, now)
            .expect("begin submit");
        record
            .transition(OrderPathEvent::SubmitAcceptedWithoutBrokerOrderId, now)
            .expect("accepted without broker id");

        assert_eq!(record.state, OrderPathState::SubmittedPendingBrokerOrderId);
        assert_eq!(
            record.last_ack_status,
            Some(CommandAckStatus::UnknownPending)
        );
        assert_eq!(
            record.last_error_kind,
            Some(OrderPathErrorKind::ReconciliationRequired)
        );
        assert!(matches!(
            record.transition(OrderPathEvent::RequestCancel, now),
            Err(OrderPathTransitionError::InvalidTransition {
                from: OrderPathState::SubmittedPendingBrokerOrderId,
                event: OrderPathEvent::RequestCancel
            })
        ));

        record.broker_order_id = Some(BrokerOrderId::new("BROKER_TEST_49"));
        record
            .transition(OrderPathEvent::RecoverByClientOrderId, now)
            .expect("recover broker id by client id");
        record
            .transition(OrderPathEvent::RequestCancel, now)
            .expect("cancel after recovery");
        assert_eq!(record.state, OrderPathState::CancelRequested);
    }

    #[test]
    fn recovery_helper_sets_broker_id_once_and_allows_cancel_preflight() {
        let now = Utc::now();
        let order = place_order(request_id(50), "CID000000000000050");
        let mut record = OrderPathRecord::from_place_order(&order, now, None);
        record
            .transition(OrderPathEvent::BeginSubmit, now)
            .expect("begin submit");
        record
            .transition(OrderPathEvent::SubmitAcceptedWithoutBrokerOrderId, now)
            .expect("accepted without broker id");
        let mut store = InMemoryOrderPathStore::default();
        store.insert_intent(record).expect("insert pending record");

        let recovered = recover_order_path_by_client_order_id(
            &mut store,
            &order.client_order_id,
            BrokerOrderId::new("BROKER_TEST_RECOVERED_50"),
            now + chrono::Duration::milliseconds(1),
        )
        .expect("recover by broker truth client id match");

        assert_eq!(recovered.state, OrderPathState::RecoveredByClientOrderId);
        assert_eq!(
            recovered.broker_order_id,
            Some(BrokerOrderId::new("BROKER_TEST_RECOVERED_50"))
        );
        assert_eq!(
            recovered.last_reconciliation_source,
            Some(OrderPathReconciliationSource::ClientOrderId)
        );
        assert_eq!(
            store
                .load_by_broker_order_id(&BrokerOrderId::new("BROKER_TEST_RECOVERED_50"))
                .expect("broker id index")
                .request_id,
            order.request_id
        );

        let cancel = cancel_order(request_id(51), "BROKER_TEST_RECOVERED_50");
        let stored = store
            .load_by_request_id(order.request_id)
            .expect("stored recovered record");
        assert!(matches!(
            preflight_policy(now)
                .approve_cancel_order(&cancel, now, Some(&stored))
                .expect("cancel preflight after recovery"),
            CancelPreflightApproval::Submit(_)
        ));

        let idempotent_recovery = recover_order_path_by_client_order_id(
            &mut store,
            &order.client_order_id,
            BrokerOrderId::new("BROKER_TEST_RECOVERED_50"),
            now + chrono::Duration::milliseconds(2),
        )
        .expect("same broker truth fact is idempotent");
        assert_eq!(idempotent_recovery, recovered);

        let mismatched_recovery = recover_order_path_by_client_order_id(
            &mut store,
            &order.client_order_id,
            BrokerOrderId::new("BROKER_TEST_OTHER_50"),
            now + chrono::Duration::milliseconds(3),
        )
        .expect_err("different broker id for same client id must fail");
        assert_eq!(
            mismatched_recovery,
            OrderPathReconciliationError::BrokerOrderIdMismatch(order.request_id)
        );
    }

    #[test]
    fn recovery_helper_rejects_duplicate_broker_truth_id() {
        let now = Utc::now();
        let first_order = place_order(request_id(52), "CID000000000000052");
        let mut first = OrderPathRecord::from_place_order(&first_order, now, None);
        first.broker_order_id = Some(BrokerOrderId::new("BROKER_TEST_DUP_RECOVERY"));
        first
            .transition(OrderPathEvent::BeginSubmit, now)
            .expect("first begin");
        first
            .transition(OrderPathEvent::SubmitAccepted, now)
            .expect("first submitted");

        let second_order = place_order(request_id(53), "CID000000000000053");
        let mut second = OrderPathRecord::from_place_order(&second_order, now, None);
        second
            .transition(OrderPathEvent::BeginSubmit, now)
            .expect("second begin");
        second
            .transition(OrderPathEvent::SubmitAcceptedWithoutBrokerOrderId, now)
            .expect("second accepted without id");

        let mut store = InMemoryOrderPathStore::default();
        store.insert_intent(first).expect("insert first");
        store.insert_intent(second).expect("insert second");

        let error = recover_order_path_by_client_order_id(
            &mut store,
            &second_order.client_order_id,
            BrokerOrderId::new("BROKER_TEST_DUP_RECOVERY"),
            now + chrono::Duration::milliseconds(1),
        )
        .expect_err("duplicate broker truth id must be rejected");

        assert!(matches!(
            error,
            OrderPathReconciliationError::Store(OrderPathStoreError::DuplicateBrokerOrderId(_))
        ));
        assert!(store
            .load_by_request_id(second_order.request_id)
            .expect("second still pending")
            .broker_order_id
            .is_none());
    }

    #[test]
    fn order_path_cancel_state_machine_handles_recovered_and_timeout_without_blind_retry() {
        let now = Utc::now();
        let order = place_order(request_id(19), "CID000000000000019");
        let mut recovered = OrderPathRecord::from_place_order(&order, now, None);
        recovered
            .transition(OrderPathEvent::BeginSubmit, now)
            .expect("begin");
        recovered
            .transition(OrderPathEvent::SubmitTimedOut, now)
            .expect("timeout");
        recovered
            .transition(OrderPathEvent::RecoverByClientOrderId, now)
            .expect("recover active by client id");
        recovered
            .transition(OrderPathEvent::RequestCancel, now)
            .expect("cancel recovered active order");
        assert_eq!(recovered.state, OrderPathState::CancelRequested);

        recovered
            .transition(OrderPathEvent::CancelTimedOut, now)
            .expect("cancel timeout");
        assert_eq!(recovered.state, OrderPathState::CancelTimeoutUnknownPending);
        assert_eq!(recovered.last_ack_status, Some(CommandAckStatus::Timeout));

        let blind_retry = recovered
            .transition(OrderPathEvent::RequestCancel, now)
            .expect_err("cancel blind retry blocked");
        assert_eq!(
            blind_retry,
            OrderPathTransitionError::InvalidTransition {
                from: OrderPathState::CancelTimeoutUnknownPending,
                event: OrderPathEvent::RequestCancel
            }
        );

        let mut recovered_terminal = recovered.clone();
        recovered_terminal
            .transition(OrderPathEvent::RecoverCancelTerminal, now)
            .expect("cancel recovered terminal by broker truth");
        assert_eq!(
            recovered_terminal.state,
            OrderPathState::CancelRecoveredTerminal
        );
        assert_eq!(
            recovered_terminal.last_reconciliation_source,
            Some(OrderPathReconciliationSource::OrderSnapshot)
        );

        let mut manual = recovered;
        manual
            .transition(OrderPathEvent::RequireManualIntervention, now)
            .expect("manual intervention after bounded reconciliation");
        assert_eq!(manual.state, OrderPathState::ManualInterventionRequired);

        let mut cancel_rejected = OrderPathRecord::from_place_order(&order, now, None);
        cancel_rejected.broker_order_id = Some(BrokerOrderId::new("BROKER_TEST_CANCEL_REJECT"));
        cancel_rejected
            .transition(OrderPathEvent::BeginSubmit, now)
            .expect("begin");
        cancel_rejected
            .transition(OrderPathEvent::SubmitAccepted, now)
            .expect("submitted");
        cancel_rejected
            .transition(OrderPathEvent::RequestCancel, now)
            .expect("cancel requested");
        cancel_rejected
            .transition(OrderPathEvent::CancelRejected, now)
            .expect("cancel rejected");
        assert_eq!(
            cancel_rejected.state,
            OrderPathState::ManualInterventionRequired
        );
        assert_eq!(
            cancel_rejected.last_error_kind,
            Some(OrderPathErrorKind::BrokerRejected)
        );
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
    fn order_path_store_rejects_duplicate_broker_order_ids_and_indexes_lookup() {
        let now = Utc::now();
        let mut first = OrderPathRecord::from_place_order(
            &place_order(request_id(16), "CID000000000000016"),
            now,
            None,
        );
        first.broker_order_id = Some(BrokerOrderId::new("BROKER_TEST_DUP"));
        let mut duplicate = OrderPathRecord::from_place_order(
            &place_order(request_id(17), "CID000000000000017"),
            now,
            None,
        );
        duplicate.broker_order_id = Some(BrokerOrderId::new("BROKER_TEST_DUP"));
        let mut store = InMemoryOrderPathStore::default();

        store.insert_intent(first.clone()).expect("first insert");
        assert_eq!(
            store
                .load_by_broker_order_id(&BrokerOrderId::new("BROKER_TEST_DUP"))
                .expect("broker lookup")
                .request_id,
            first.request_id
        );
        assert!(matches!(
            store.insert_intent(duplicate),
            Err(OrderPathStoreError::DuplicateBrokerOrderId(_))
        ));

        let mut second = OrderPathRecord::from_place_order(
            &place_order(request_id(18), "CID000000000000018"),
            now,
            None,
        );
        store.insert_intent(second.clone()).expect("second insert");
        second.broker_order_id = Some(BrokerOrderId::new("BROKER_TEST_DUP"));
        assert!(matches!(
            store.update_record(second),
            Err(OrderPathStoreError::DuplicateBrokerOrderId(_))
        ));
    }

    #[test]
    fn order_path_store_rejects_broker_id_changes_terminal_overwrite_and_timestamp_regression() {
        let now = Utc::now();
        let request_id = request_id(29);
        let order = place_order(request_id, "CID000000000000029");
        let mut record = OrderPathRecord::from_place_order(&order, now, None);
        let mut store = InMemoryOrderPathStore::default();
        store.insert_intent(record.clone()).expect("insert");

        record.broker_order_id = Some(BrokerOrderId::new("BROKER_TEST_29"));
        store
            .update_record(record.clone())
            .expect("first broker id mapping can be recorded");

        let mut cleared_broker_id = record.clone();
        cleared_broker_id.broker_order_id = None;
        assert!(matches!(
            store.update_record(cleared_broker_id),
            Err(OrderPathStoreError::BrokerOrderIdChanged(_))
        ));

        let mut changed_broker_id = record.clone();
        changed_broker_id.broker_order_id = Some(BrokerOrderId::new("BROKER_TEST_OTHER"));
        assert!(matches!(
            store.update_record(changed_broker_id),
            Err(OrderPathStoreError::BrokerOrderIdChanged(_))
        ));

        let mut regressed_timestamp = record.clone();
        regressed_timestamp.last_update_ts = now - chrono::Duration::milliseconds(1);
        assert!(matches!(
            store.update_record(regressed_timestamp),
            Err(OrderPathStoreError::UpdatedTimestampRegressed(_))
        ));

        let mut terminal = record.clone();
        terminal
            .transition(
                OrderPathEvent::LocalReject,
                now + chrono::Duration::seconds(1),
            )
            .expect("reject");
        terminal
            .transition(
                OrderPathEvent::MarkTerminal,
                now + chrono::Duration::seconds(2),
            )
            .expect("terminal");
        store.update_record(terminal.clone()).expect("terminal");

        let mut overwrite_terminal = terminal.clone();
        overwrite_terminal.state = OrderPathState::Submitted;
        overwrite_terminal.last_update_ts = terminal.last_update_ts + chrono::Duration::seconds(1);
        assert!(matches!(
            store.update_record(overwrite_terminal),
            Err(OrderPathStoreError::TerminalStateOverwrite(_))
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
    fn json_file_order_path_store_rejects_duplicate_broker_ids_after_restart() {
        let now = Utc::now();
        let path = temp_store_path("duplicate_broker");
        let mut first = OrderPathRecord::from_place_order(
            &place_order(request_id(33), "CID000000000000033"),
            now,
            None,
        );
        first.broker_order_id = Some(BrokerOrderId::new("BROKER_TEST_REOPEN_DUP"));
        let mut second = OrderPathRecord::from_place_order(
            &place_order(request_id(34), "CID000000000000034"),
            now,
            None,
        );
        second.broker_order_id = Some(BrokerOrderId::new("BROKER_TEST_REOPEN_DUP"));
        let snapshot = OrderPathStoreSnapshot {
            records: vec![first, second],
        };
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&snapshot).expect("snapshot"),
        )
        .expect("write duplicate snapshot");

        assert!(matches!(
            JsonFileOrderPathStore::open(&path),
            Err(OrderPathStoreError::DuplicateBrokerOrderId(_))
        ));

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
    fn sqlite_order_path_store_uses_wal_and_single_writer_lock() {
        let path = temp_store_path("sqlite_wal");
        cleanup_sqlite_store(&path);
        let store = SqliteOrderPathStore::open(&path).expect("open sqlite store");
        let lock_metadata = store.writer_lock_metadata().clone();

        let journal_mode: String = store
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("journal mode");
        let synchronous: i64 = store
            .connection
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .expect("synchronous");
        assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
        assert_eq!(synchronous, 2);
        assert!(store.lock_path().exists());
        let raw_lock = std::fs::read_to_string(store.lock_path()).expect("lock metadata");
        let parsed_lock: SqliteWriterLockMetadata =
            serde_json::from_str(&raw_lock).expect("lock metadata parses");
        assert_eq!(parsed_lock, lock_metadata);
        assert_eq!(parsed_lock.pid, std::process::id());
        assert_eq!(parsed_lock.schema_version, SQLITE_ORDER_PATH_SCHEMA_VERSION);
        assert!(!parsed_lock.instance_id.is_empty());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mode = std::fs::metadata(store.path())
                .expect("db metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0);
        }
        assert!(matches!(
            SqliteOrderPathStore::open(&path),
            Err(OrderPathStoreError::WriterLockUnavailable(_))
        ));
        let second_writer_error = match SqliteOrderPathStore::open(&path) {
            Ok(_) => panic!("second writer must be rejected"),
            Err(error) => error,
        };
        assert!(format!("{second_writer_error}").contains("pid="));

        drop(store);
        assert!(!sqlite_writer_lock_path(&path).exists());
        cleanup_sqlite_store(&path);
    }

    #[test]
    fn sqlite_order_path_store_does_not_auto_remove_stale_lock_and_cleans_failed_open() {
        let path = temp_store_path("sqlite_stale_lock");
        cleanup_sqlite_store(&path);
        std::fs::write(sqlite_writer_lock_path(&path), "stale lock").expect("write stale lock");
        assert!(matches!(
            SqliteOrderPathStore::open(&path),
            Err(OrderPathStoreError::WriterLockUnavailable(_))
        ));
        assert!(sqlite_writer_lock_path(&path).exists());
        cleanup_sqlite_store(&path);

        let dir_path = temp_store_path("sqlite_open_failure_dir");
        cleanup_sqlite_store(&dir_path);
        std::fs::create_dir_all(&dir_path).expect("create directory where db file is expected");
        assert!(matches!(
            SqliteOrderPathStore::open(&dir_path),
            Err(OrderPathStoreError::Sqlite(_))
        ));
        assert!(!sqlite_writer_lock_path(&dir_path).exists());
        let _ = std::fs::remove_dir_all(&dir_path);
    }

    #[test]
    fn sqlite_order_path_store_blocks_schema_version_mismatch() {
        let path = temp_store_path("sqlite_schema_mismatch");
        cleanup_sqlite_store(&path);
        {
            let connection = Connection::open(&path).expect("open raw sqlite");
            connection
                .execute_batch(
                    r#"
                    CREATE TABLE order_path_schema (
                        key TEXT PRIMARY KEY,
                        value INTEGER NOT NULL
                    );
                    INSERT INTO order_path_schema (key, value)
                    VALUES ('schema_version', 999);
                    "#,
                )
                .expect("seed newer schema");
        }

        let schema_error = match SqliteOrderPathStore::open(&path) {
            Ok(_) => panic!("schema mismatch must block writer open"),
            Err(error) => error,
        };
        assert_eq!(
            schema_error,
            OrderPathStoreError::SchemaVersionMismatch {
                expected: SQLITE_ORDER_PATH_SCHEMA_VERSION,
                found: 999,
            }
        );
        assert!(!sqlite_writer_lock_path(&path).exists());
        cleanup_sqlite_store(&path);
    }

    #[test]
    fn sqlite_order_path_store_persists_unique_records_and_reopens() {
        let now = Utc::now();
        let path = temp_store_path("sqlite_persist");
        cleanup_sqlite_store(&path);
        let order = place_order(request_id(54), "CID000000000000054");
        let mut record = OrderPathRecord::from_place_order(&order, now, None);
        record
            .transition(OrderPathEvent::BeginSubmit, now)
            .expect("begin submit before persist");

        {
            let mut store = SqliteOrderPathStore::open(&path).expect("open sqlite store");
            store.insert_intent(record.clone()).expect("insert record");
            assert!(matches!(
                store.insert_intent(OrderPathRecord::from_place_order(&order, now, None)),
                Err(OrderPathStoreError::DuplicateStrategyRequestId(_))
            ));
        }
        {
            let mut store = SqliteOrderPathStore::open(&path).expect("reopen sqlite store");
            let mut loaded = store
                .load_by_request_id(order.request_id)
                .expect("record survives reopen");
            assert_eq!(loaded.state, OrderPathState::SubmitInFlight);
            loaded
                .recover_after_restart(now + chrono::Duration::seconds(1))
                .expect("recover inflight after restart");
            store
                .update_record(loaded.clone())
                .expect("persist recovered state");
            let audit = store.transition_audit().expect("audit entries");
            assert_eq!(audit.len(), 2);
            assert_eq!(audit[0].event, "InsertIntent");
            assert_eq!(audit[0].from_state, None);
            assert_eq!(audit[0].to_state, "SubmitInFlight");
            assert_eq!(audit[1].event, "SubmitTimedOut");
            assert_eq!(audit[1].from_state.as_deref(), Some("SubmitInFlight"));
            assert_eq!(audit[1].to_state, "TimeoutUnknownPending");
            #[cfg(unix)]
            assert_existing_sqlite_runtime_files_are_protected(&path);
        }
        {
            let store =
                SqliteOrderPathReadStore::open_readonly(&path).expect("open read-only diagnostics");
            assert_eq!(
                store
                    .operator_load_by_client_order_id(&order.client_order_id)
                    .expect("client id lookup")
                    .state,
                OrderPathState::TimeoutUnknownPending
            );
            assert_eq!(store.operator_all_records().len(), 1);
            assert_eq!(store.transition_audit().expect("read audit").len(), 2);
            assert!(store
                .connection
                .execute(
                    "INSERT INTO order_path_records \
                     (request_id, client_order_id, state, last_update_ts, payload) \
                     VALUES ('REQ_TEST', 'CID_TEST', 'IntentRecorded', 'ts', '{}')",
                    [],
                )
                .is_err());
        }

        cleanup_sqlite_store(&path);
    }

    #[test]
    fn sqlite_runtime_directory_inspector_flags_deployment_issues() {
        let private_dir = temp_runtime_dir("private_runtime");
        std::fs::create_dir_all(&private_dir).expect("create private dir");
        #[cfg(unix)]
        chmod(&private_dir, 0o700);
        assert!(inspect_sqlite_runtime_directory(&private_dir, None).is_empty());

        let missing_dir = private_dir.join("missing");
        assert_eq!(
            inspect_sqlite_runtime_directory(&missing_dir, None),
            vec![SqliteRuntimeDirectoryIssue::Missing]
        );

        let not_dir = private_dir.join("not_dir");
        std::fs::write(&not_dir, "not a directory").expect("write file");
        assert_eq!(
            inspect_sqlite_runtime_directory(&not_dir, None),
            vec![SqliteRuntimeDirectoryIssue::NotDirectory]
        );

        let workspace = temp_runtime_dir("workspace");
        let artifact_runtime = workspace.join("reports").join("handoff_runtime");
        std::fs::create_dir_all(&artifact_runtime).expect("create artifact runtime dir");
        #[cfg(unix)]
        chmod(&artifact_runtime, 0o755);
        let issues = inspect_sqlite_runtime_directory(&artifact_runtime, Some(&workspace));
        assert!(issues.contains(&SqliteRuntimeDirectoryIssue::InsideWorkspaceTree));
        assert!(issues.contains(&SqliteRuntimeDirectoryIssue::InsideWorkspaceArtifactArea));
        #[cfg(unix)]
        assert!(issues.contains(&SqliteRuntimeDirectoryIssue::GroupOrWorldAccessible));

        let _ = std::fs::remove_dir_all(private_dir);
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn sqlite_order_path_store_preserves_cancel_and_pending_states_after_reopen() {
        let now = Utc::now();
        let path = temp_store_path("sqlite_reopen_states");
        cleanup_sqlite_store(&path);

        let cancel_order_place = place_order(request_id(55), "CID000000000000055");
        let mut cancel_requested =
            OrderPathRecord::from_place_order(&cancel_order_place, now, None);
        cancel_requested.broker_order_id = Some(BrokerOrderId::new("BROKER_TEST_SQLITE_55"));
        cancel_requested
            .transition(OrderPathEvent::BeginSubmit, now)
            .expect("begin");
        cancel_requested
            .transition(OrderPathEvent::SubmitAccepted, now)
            .expect("submit accepted");
        cancel_requested
            .transition(
                OrderPathEvent::RequestCancel,
                now + chrono::Duration::milliseconds(1),
            )
            .expect("request cancel");

        let pending_order = place_order(request_id(56), "CID000000000000056");
        let mut submitted_pending = OrderPathRecord::from_place_order(&pending_order, now, None);
        submitted_pending
            .transition(OrderPathEvent::BeginSubmit, now)
            .expect("begin pending");
        submitted_pending
            .transition(
                OrderPathEvent::SubmitAcceptedWithoutBrokerOrderId,
                now + chrono::Duration::milliseconds(1),
            )
            .expect("accepted without id");

        {
            let mut store = SqliteOrderPathStore::open(&path).expect("open sqlite store");
            store
                .insert_intent(cancel_requested.clone())
                .expect("insert cancel requested");
            store
                .insert_intent(submitted_pending.clone())
                .expect("insert submitted pending");
        }
        {
            let store = SqliteOrderPathStore::open(&path).expect("reopen sqlite store");
            assert_eq!(
                store
                    .load_by_request_id(cancel_requested.request_id)
                    .expect("cancel requested")
                    .state,
                OrderPathState::CancelRequested
            );
            assert_eq!(
                store
                    .load_by_request_id(submitted_pending.request_id)
                    .expect("submitted pending")
                    .state,
                OrderPathState::SubmittedPendingBrokerOrderId
            );
        }

        cleanup_sqlite_store(&path);
    }

    #[test]
    fn sqlite_order_path_store_redacted_export_omits_raw_ids() {
        let now = Utc::now();
        let path = temp_store_path("sqlite_redacted");
        cleanup_sqlite_store(&path);
        let order = place_order(request_id(57), "CID000000000000057");
        let mut record = OrderPathRecord::from_place_order(&order, now, None);
        record.broker_order_id = Some(BrokerOrderId::new("BROKER_TEST_SQLITE_57"));
        record
            .transition(OrderPathEvent::BeginSubmit, now)
            .expect("begin");
        record
            .transition(OrderPathEvent::SubmitAccepted, now)
            .expect("submitted");

        let mut store = SqliteOrderPathStore::open(&path).expect("open sqlite store");
        store.insert_intent(record).expect("insert record");
        let redacted_json =
            serde_json::to_string(&store.redacted_records()).expect("redacted export serializes");

        assert!(redacted_json.contains("client_order_id_fingerprint"));
        assert!(redacted_json.contains("broker_order_id_fingerprint"));
        assert!(!redacted_json.contains("CID000000000000057"));
        assert!(!redacted_json.contains("BROKER_TEST_SQLITE_57"));
        assert!(!redacted_json.contains("ACC_TEST_0001"));

        drop(store);
        cleanup_sqlite_store(&path);
    }

    #[test]
    fn sqlite_order_path_store_blocks_corrupt_database() {
        let path = temp_store_path("sqlite_corrupt");
        cleanup_sqlite_store(&path);
        std::fs::write(&path, "not a sqlite database").expect("write corrupt file");

        assert!(matches!(
            SqliteOrderPathStore::open(&path),
            Err(OrderPathStoreError::Sqlite(_))
        ));
        assert!(!sqlite_writer_lock_path(&path).exists());

        cleanup_sqlite_store(&path);
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
            Some(CommandAckReason::synthetic_submitted()),
            now,
        );

        assert_eq!(ack.request_id, record.request_id);
        assert_eq!(ack.client_order_id, Some(record.client_order_id));
        assert_eq!(
            ack.broker_order_id,
            Some(BrokerOrderId::new("BROKER_TEST_1"))
        );
        assert_eq!(ack.status, CommandAckStatus::Submitted);
        assert_eq!(ack.reason, Some(CommandAckReason::synthetic_submitted()));
        let rendered = serde_json::to_string(&ack).expect("ack serializes");
        assert!(rendered.contains("synthetic_submitted"));
        assert!(!rendered.contains("raw broker response"));
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
        assert_eq!(
            arm.validate(now).expect_err("one-shot already used"),
            OrderPreflightError::OneShotAlreadyUsed
        );

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
    fn operator_arm_disarms_on_m3_safety_signals() {
        let now = Utc::now();
        for signal in [
            OperatorDisarmSignal::GatewayDegraded,
            OperatorDisarmSignal::OrderPathStoreLockUncertain,
            OperatorDisarmSignal::OrderPathStoreMigrationMismatch,
            OperatorDisarmSignal::OrderPathStoreUnavailable,
            OperatorDisarmSignal::RuntimeBridgeDeadLetter,
            OperatorDisarmSignal::UnknownPendingOrder,
            OperatorDisarmSignal::AcceptedWithoutBrokerOrderId,
            OperatorDisarmSignal::CancelBrokerOrderIdMismatch,
            OperatorDisarmSignal::CancelTimeoutUnknownPending,
            OperatorDisarmSignal::OrderEndpointRateLimited,
            OperatorDisarmSignal::OrderEndpointMaintenance,
            OperatorDisarmSignal::OrderEndpointDecodeError,
            OperatorDisarmSignal::OrderEndpointUnauthorized,
            OperatorDisarmSignal::ReconciliationStale,
            OperatorDisarmSignal::RestartRecovery,
        ] {
            let mut arm = sample_arm(now);

            let decision = arm.disarm_for_safety_signal(signal);

            assert_eq!(decision.signal, signal);
            assert!(decision.was_enabled);
            assert!(!decision.endpoint_calls_enabled);
            assert_eq!(
                arm.validate(now).expect_err("disarmed arm must block"),
                OrderPreflightError::EndpointNotArmed
            );
        }
    }

    #[test]
    fn operator_arm_can_be_rearmed_after_operator_visible_safety_disarm() {
        let now = Utc::now();
        let mut arm = sample_arm(now);

        arm.disarm_for_safety_signal(OperatorDisarmSignal::RuntimeBridgeDeadLetter);
        assert_eq!(
            arm.validate(now).expect_err("disarmed"),
            OrderPreflightError::EndpointNotArmed
        );

        arm.rearm(
            "ARM_TEST_2",
            now + chrono::Duration::minutes(10),
            "digest-test-2",
            true,
        );

        arm.validate(now).expect("rearmed arm is valid");
        arm.record_endpoint_attempt();
        assert_eq!(
            arm.validate(now).expect_err("one-shot consumed"),
            OrderPreflightError::OneShotAlreadyUsed
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
    fn preflight_approval_markers_are_returned_only_after_validation() {
        let now = Utc::now();
        let policy = preflight_policy(now);
        let order = place_order(request_id(45), "CID000000000000045");

        let approved_place = policy
            .approve_place_order(&order, now)
            .expect("place approved");

        assert_eq!(approved_place.order().request_id, order.request_id);
        assert_eq!(approved_place.approved_ts(), now);

        let mut existing = OrderPathRecord::from_place_order(&order, now, None);
        existing.broker_order_id = Some(BrokerOrderId::new("BROKER_TEST_45"));
        existing
            .transition(OrderPathEvent::BeginSubmit, now)
            .expect("begin submit");
        existing
            .transition(OrderPathEvent::SubmitAccepted, now)
            .expect("submitted");
        let cancel = cancel_order(request_id(46), "BROKER_TEST_45");

        let approved_cancel = policy
            .approve_cancel_order(&cancel, now, Some(&existing))
            .expect("cancel approved");

        match approved_cancel {
            CancelPreflightApproval::Submit(approved) => {
                assert_eq!(approved.cancel().request_id, cancel.request_id);
                assert_eq!(approved.mapped_request_id(), Some(order.request_id));
                assert_eq!(approved.approved_ts(), now);
            }
            CancelPreflightApproval::AlreadyTerminal => panic!("expected submit approval"),
        }
    }

    #[test]
    fn preflight_rejects_expired_place_and_cancel_commands() {
        let now = Utc::now();
        let policy = preflight_policy(now);
        let mut order = place_order(request_id(47), "CID000000000000047");
        order.created_ts = now - chrono::Duration::milliseconds(1_001);
        order.ttl_ms = Some(1_000);

        assert_eq!(
            policy
                .approve_place_order(&order, now)
                .expect_err("expired place command"),
            OrderPreflightError::CommandExpired
        );

        let mut cancel = cancel_order(request_id(48), "BROKER_TEST_48");
        cancel.created_ts = now - chrono::Duration::milliseconds(1_001);
        cancel.ttl_ms = Some(1_000);

        assert_eq!(
            policy
                .approve_cancel_order(&cancel, now, None)
                .expect_err("expired cancel command"),
            OrderPreflightError::CommandExpired
        );
    }

    #[test]
    fn dry_order_rate_limit_consumes_capacity_without_overdraft() {
        let mut rate_limit = DryOrderRateLimit::new(2);

        rate_limit.try_consume(0).expect("zero permit no-op");
        assert_eq!(rate_limit.remaining(), 2);
        rate_limit.try_consume(1).expect("first permit");
        assert_eq!(rate_limit.remaining(), 1);
        rate_limit.try_consume(1).expect("second permit");
        assert_eq!(rate_limit.remaining(), 0);
        assert_eq!(
            rate_limit.try_consume(1).expect_err("capacity exhausted"),
            DryOrderRateLimitError::CapacityExhausted
        );
        assert_eq!(rate_limit.used, 2);
    }

    #[test]
    fn dry_order_rate_window_resets_and_respects_backoff() {
        let now = Utc::now();
        let mut window = DryOrderRateWindow::new(2, 1_000, now);

        assert_eq!(
            window.try_consume(now, 1).expect("first permit").remaining,
            1
        );
        assert_eq!(
            window
                .try_consume(now + chrono::Duration::milliseconds(100), 1)
                .expect("second permit")
                .remaining,
            0
        );
        assert_eq!(
            window
                .try_consume(now + chrono::Duration::milliseconds(200), 1)
                .expect_err("window capacity exhausted"),
            DryOrderRateWindowError::CapacityExhausted {
                reset_after_ms: 800
            }
        );
        assert_eq!(
            window
                .try_consume(now + chrono::Duration::milliseconds(1_000), 1)
                .expect("window reset")
                .remaining,
            1
        );

        let mut backoff = DryOrderRateWindow::new(2, 1_000, now)
            .with_backoff_until(now + chrono::Duration::milliseconds(500));
        assert_eq!(
            backoff
                .try_consume(now + chrono::Duration::milliseconds(250), 1)
                .expect_err("backoff active"),
            DryOrderRateWindowError::BackoffActive {
                retry_after_ms: 250
            }
        );
        assert_eq!(
            backoff
                .try_consume(now + chrono::Duration::milliseconds(500), 1)
                .expect("backoff elapsed")
                .remaining,
            1
        );
    }

    #[test]
    fn preflight_rejects_raw_place_order_comment_at_command_boundary() {
        let now = Utc::now();
        let policy = preflight_policy(now);
        let mut order = place_order(request_id(15), "CID000000000000015");
        order.comment = Some("raw broker comment must not enter command path".to_string());

        assert_eq!(
            policy
                .validate_place_order(&order, now)
                .expect_err("raw comment"),
            OrderPreflightError::RawCommandCommentNotAllowed
        );
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

        let mut recovered_active = existing.clone();
        recovered_active.state = OrderPathState::RecoveredByClientOrderId;
        recovered_active.last_update_ts = now + chrono::Duration::milliseconds(1);
        assert_eq!(
            policy
                .validate_cancel_order(&cancel, now, Some(&recovered_active))
                .expect("cancel recovered active"),
            CancelPreflightDecision::SubmitCancel
        );

        let mut pending_cancel = existing.clone();
        pending_cancel.state = OrderPathState::CancelRequested;
        pending_cancel.last_update_ts = now + chrono::Duration::milliseconds(2);
        assert_eq!(
            policy
                .validate_cancel_order(&cancel, now, Some(&pending_cancel))
                .expect_err("cancel already pending"),
            OrderPreflightError::CancelAlreadyPending
        );

        let mut unknown_place = existing.clone();
        unknown_place.state = OrderPathState::TimeoutUnknownPending;
        unknown_place.last_update_ts = now + chrono::Duration::milliseconds(3);
        assert_eq!(
            policy
                .validate_cancel_order(&cancel, now, Some(&unknown_place))
                .expect_err("place unknown requires manual"),
            OrderPreflightError::CancelStateRequiresManualIntervention
        );

        let mut accepted_without_broker_id = existing.clone();
        accepted_without_broker_id.state = OrderPathState::SubmittedPendingBrokerOrderId;
        accepted_without_broker_id.last_update_ts = now + chrono::Duration::milliseconds(4);
        assert_eq!(
            policy
                .validate_cancel_order(&cancel, now, Some(&accepted_without_broker_id))
                .expect_err("accepted without broker id requires recovery"),
            OrderPreflightError::CancelStateRequiresManualIntervention
        );

        let mut unknown_cancel = existing.clone();
        unknown_cancel.state = OrderPathState::CancelTimeoutUnknownPending;
        unknown_cancel.last_update_ts = now + chrono::Duration::milliseconds(5);
        assert_eq!(
            policy
                .validate_cancel_order(&cancel, now, Some(&unknown_cancel))
                .expect_err("cancel unknown requires manual"),
            OrderPreflightError::CancelStateRequiresManualIntervention
        );

        let mut missing_broker_mapping = existing.clone();
        missing_broker_mapping.broker_order_id = None;
        assert_eq!(
            policy
                .validate_cancel_order(&cancel, now, Some(&missing_broker_mapping))
                .expect_err("missing broker mapping"),
            OrderPreflightError::CancelMappingMissing
        );

        let mismatched_cancel = cancel_order(request_id(44), "BROKER_TEST_OTHER");
        assert_eq!(
            policy
                .validate_cancel_order(&mismatched_cancel, now, Some(&existing))
                .expect_err("mismatched mapping"),
            OrderPreflightError::CancelMappingMismatch
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
                .validate_cancel_order(&mismatched_cancel, now, Some(&existing))
                .expect_err("terminal mismatch"),
            OrderPreflightError::CancelMappingMismatch
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
    fn decimal_multiple_handles_common_futures_scales_without_rounding() {
        assert!(is_decimal_multiple(Decimal::new(10, 1), Decimal::new(1, 1)));
        assert!(is_decimal_multiple(
            Decimal::new(9_123, 2),
            Decimal::new(1, 2)
        ));
        assert!(is_decimal_multiple(
            Decimal::new(10_010, 2),
            Decimal::new(5, 2)
        ));
        assert!(!is_decimal_multiple(
            Decimal::new(10_011, 2),
            Decimal::new(5, 2)
        ));
        assert!(is_decimal_multiple(
            Decimal::new(250_000, 0),
            Decimal::new(10, 0)
        ));
        assert!(!is_decimal_multiple(Decimal::ONE, Decimal::ZERO));
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

    #[test]
    fn preflight_limit_reference_band_allows_exact_boundary_and_rejects_epsilon() {
        let now = Utc::now();
        let mut policy = preflight_policy(now);
        policy.price_step = Some(Decimal::new(1, 2));
        policy.max_limit_deviation_bps = Some(100);

        let mut exact_boundary = place_order(request_id(74), "CID000000000000074");
        exact_boundary.limit_price = Some(Decimal::new(101_000, 2));
        policy
            .validate_place_order_with_context(
                &exact_boundary,
                now,
                &fresh_reference(now, Decimal::new(100_000, 2)),
            )
            .expect("exact bps boundary is allowed");

        let mut over_boundary = place_order(request_id(75), "CID000000000000075");
        over_boundary.limit_price = Some(Decimal::new(101_001, 2));
        assert_eq!(
            policy
                .validate_place_order_with_context(
                    &over_boundary,
                    now,
                    &fresh_reference(now, Decimal::new(100_000, 2))
                )
                .expect_err("over boundary"),
            OrderPreflightError::LimitPriceBandExceeded
        );

        assert_eq!(
            policy
                .validate_place_order_with_context(
                    &exact_boundary,
                    now,
                    &fresh_reference(now, Decimal::ZERO)
                )
                .expect_err("zero reference"),
            OrderPreflightError::ReferencePriceInvalid
        );
    }

    #[test]
    fn order_path_store_errors_map_to_operator_disarm_signals() {
        assert_eq!(
            OrderPathStoreError::WriterLockUnavailable("present".to_string())
                .operator_disarm_signal(),
            OperatorDisarmSignal::OrderPathStoreLockUncertain
        );
        assert_eq!(
            (OrderPathStoreError::SchemaVersionMismatch {
                expected: 1,
                found: 2,
            })
            .operator_disarm_signal(),
            OperatorDisarmSignal::OrderPathStoreMigrationMismatch
        );
        assert_eq!(
            OrderPathStoreError::Sqlite("disk unavailable".to_string()).operator_disarm_signal(),
            OperatorDisarmSignal::OrderPathStoreUnavailable
        );
    }

    #[test]
    fn transition_audit_event_names_follow_contract_matrix() {
        fn record_in_state(
            state: OrderPathState,
            error_kind: Option<OrderPathErrorKind>,
        ) -> OrderPathRecord {
            let mut record = OrderPathRecord::from_place_order(
                &place_order(request_id(90), "CID000000000000090"),
                Utc::now(),
                None,
            );
            record.state = state;
            record.last_error_kind = error_kind;
            record
        }

        let cases = [
            (
                OrderPathState::IntentRecorded,
                OrderPathState::SubmitInFlight,
                None,
                "BeginSubmit",
            ),
            (
                OrderPathState::SubmitInFlight,
                OrderPathState::Submitted,
                None,
                "SubmitAccepted",
            ),
            (
                OrderPathState::SubmitInFlight,
                OrderPathState::TimeoutUnknownPending,
                Some(OrderPathErrorKind::TransportTimeout),
                "SubmitTimedOut",
            ),
            (
                OrderPathState::SubmitInFlight,
                OrderPathState::ManualInterventionRequired,
                Some(OrderPathErrorKind::RateLimited),
                "RequireManualIntervention",
            ),
            (
                OrderPathState::Submitted,
                OrderPathState::CancelRequested,
                None,
                "RequestCancel",
            ),
            (
                OrderPathState::CancelRequested,
                OrderPathState::CancelSubmitted,
                None,
                "CancelAccepted",
            ),
            (
                OrderPathState::CancelRequested,
                OrderPathState::CancelTimeoutUnknownPending,
                Some(OrderPathErrorKind::TransportTimeout),
                "CancelTimedOut",
            ),
            (
                OrderPathState::CancelRequested,
                OrderPathState::ManualInterventionRequired,
                Some(OrderPathErrorKind::BrokerRejected),
                "CancelRejected",
            ),
            (
                OrderPathState::CancelRequested,
                OrderPathState::ManualInterventionRequired,
                Some(OrderPathErrorKind::ReconciliationRequired),
                "RequireManualIntervention",
            ),
        ];

        for (from, to, error_kind, expected_event) in cases {
            let previous = record_in_state(from, None);
            let record = record_in_state(to, error_kind);
            assert_eq!(
                infer_transition_audit_event(&previous, &record),
                expected_event
            );
        }
    }
}
