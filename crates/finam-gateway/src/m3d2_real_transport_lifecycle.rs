//! M3d-2d durable lifecycle integration for the gated real transport.
//!
//! This module binds the M3d-2c real `reqwest` transport to the existing
//! broker-core order-path state machine and durable SQLite transition audit.
//! It remains local-mock only and does not add command consumer, runtime live
//! attachment, or external FINAM order calls.

use broker_core::command::CommandAckStatus;
use broker_core::{
    CommandAckReason, CommandAckReasonCode, OperatorDisarmSignal, OrderPathErrorKind,
    OrderPathEvent, OrderPathRecord, OrderPathState, OrderPathStore, OrderPathStoreError,
    OrderPathTransitionError, OutgoingOrderComment, PreflightApprovedCancelOrder,
    PreflightApprovedPlaceOrder, StrategyRequestId,
};
use broker_finam::{
    AccessToken, FinamOrderEndpointMappedResult, FinamOrderExecutionOutcome,
    FinamOrderRequestBuildError,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::m3d2_real_order_transport::{
    M3d2RealOrderEndpointTransport, M3d2RealOrderEndpointTransportError,
    M3d2RealOrderEndpointTransportExecution,
};
use crate::EndpointGateApproved;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum M3d2dLifecycleOperation {
    Place,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum M3d2dLifecycleOutcomeKind {
    Submitted,
    SubmittedPendingBrokerOrderIdReconciliation,
    BrokerRejected,
    TimeoutUnknownPending,
    CancelSubmittedPendingReconciliation,
    CancelBrokerOrderIdMismatchManualIntervention,
    CancelRejectedManualIntervention,
    CancelTimeoutUnknownPending,
    ReconciliationRequired,
    RateLimitedDisarm,
    MaintenanceDisarm,
    UnauthorizedDisarm,
    NotSent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct M3d2dAckCandidateDiagnostic {
    pub status: CommandAckStatus,
    pub reason_code: Option<CommandAckReasonCode>,
    pub client_order_id_present: bool,
    pub client_order_id_len: Option<usize>,
    pub broker_order_id_present: bool,
    pub broker_order_id_len: Option<usize>,
    pub raw_client_order_id_exported: bool,
    pub raw_broker_order_id_exported: bool,
    pub raw_account_id_exported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct M3d2dRealTransportLifecycleReport {
    pub operation: M3d2dLifecycleOperation,
    pub state: OrderPathState,
    pub outcome: M3d2dLifecycleOutcomeKind,
    pub request_sent: bool,
    pub submit_attempt_count: u32,
    pub cancel_attempt_count: u32,
    pub ack_candidate: M3d2dAckCandidateDiagnostic,
    pub reconciliation_scheduled: bool,
    pub durable_intent_present_before_send: bool,
    pub durable_begin_or_cancel_persisted_before_send: bool,
    pub durable_transition_audit_required: bool,
    pub raw_token_exported: bool,
    pub raw_path_exported: bool,
    pub raw_body_exported: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct M3d2dLifecycleTimestamps {
    pub begin_ts: DateTime<Utc>,
    pub outcome_ts: DateTime<Utc>,
}

pub async fn m3d2d_place_via_real_transport<S>(
    store: &mut S,
    transport: &M3d2RealOrderEndpointTransport,
    gate: &EndpointGateApproved,
    access_token: &AccessToken,
    approved: &PreflightApprovedPlaceOrder,
    outgoing_comment: Option<&OutgoingOrderComment>,
    timestamps: M3d2dLifecycleTimestamps,
) -> Result<M3d2dRealTransportLifecycleReport, M3d2dLifecycleError>
where
    S: OrderPathStore,
{
    let spec = broker_finam::build_place_order_request(approved, outgoing_comment)?;
    let record = m3d2d_persist_place_begin_submit(store, approved, timestamps.begin_ts)?;
    let execution = transport
        .place_order_execution(gate, access_token, &spec)
        .await;
    m3d2d_apply_place_transport_execution(store, record, execution, timestamps.outcome_ts)
}

pub async fn m3d2d_cancel_via_real_transport<S>(
    store: &mut S,
    transport: &M3d2RealOrderEndpointTransport,
    gate: &EndpointGateApproved,
    access_token: &AccessToken,
    approved: &PreflightApprovedCancelOrder,
    timestamps: M3d2dLifecycleTimestamps,
) -> Result<M3d2dRealTransportLifecycleReport, M3d2dLifecycleError>
where
    S: OrderPathStore,
{
    let spec = broker_finam::build_cancel_order_request(approved)?;
    let record = m3d2d_persist_cancel_request(store, approved, timestamps.begin_ts)?;
    let execution = transport
        .cancel_order_execution(gate, access_token, &spec)
        .await;
    m3d2d_apply_cancel_transport_execution(
        store,
        record,
        approved,
        execution,
        timestamps.outcome_ts,
    )
}

pub fn m3d2d_persist_place_begin_submit<S>(
    store: &mut S,
    approved: &PreflightApprovedPlaceOrder,
    begin_ts: DateTime<Utc>,
) -> Result<OrderPathRecord, M3d2dLifecycleError>
where
    S: OrderPathStore,
{
    let request_id = approved.order().request_id;
    let mut record = store
        .load_by_request_id(request_id)
        .ok_or(M3d2dLifecycleError::MissingOrderPathRecord { request_id })?;
    record.transition(OrderPathEvent::BeginSubmit, begin_ts)?;
    store.update_record(record.clone())?;
    Ok(record)
}

pub fn m3d2d_persist_cancel_request<S>(
    store: &mut S,
    approved: &PreflightApprovedCancelOrder,
    begin_ts: DateTime<Utc>,
) -> Result<OrderPathRecord, M3d2dLifecycleError>
where
    S: OrderPathStore,
{
    let cancel_request_id = approved.cancel().request_id;
    let mapped_request_id =
        approved
            .mapped_request_id()
            .ok_or(M3d2dLifecycleError::MissingCancelMapping {
                request_id: cancel_request_id,
            })?;
    let mut record = store.load_by_request_id(mapped_request_id).ok_or(
        M3d2dLifecycleError::MissingOrderPathRecord {
            request_id: mapped_request_id,
        },
    )?;
    record.transition(OrderPathEvent::RequestCancel, begin_ts)?;
    store.update_record(record.clone())?;
    Ok(record)
}

pub fn m3d2d_apply_place_transport_execution<S>(
    store: &mut S,
    mut record: OrderPathRecord,
    execution: M3d2RealOrderEndpointTransportExecution,
    outcome_ts: DateTime<Utc>,
) -> Result<M3d2dRealTransportLifecycleReport, M3d2dLifecycleError>
where
    S: OrderPathStore,
{
    let request_sent = execution.request_sent;
    let (ack_status, reason_code, outcome) = match execution.classified_response {
        Some(classified) => match classified.result {
            FinamOrderEndpointMappedResult::Execution(FinamOrderExecutionOutcome::Accepted {
                broker_order_id: Some(broker_order_id),
            }) => {
                record.broker_order_id = Some(broker_order_id);
                record.transition(OrderPathEvent::SubmitAccepted, outcome_ts)?;
                (
                    CommandAckStatus::Submitted,
                    Some(CommandAckReasonCode::SyntheticSubmitted),
                    M3d2dLifecycleOutcomeKind::Submitted,
                )
            }
            FinamOrderEndpointMappedResult::Execution(FinamOrderExecutionOutcome::Accepted {
                broker_order_id: None,
            }) => {
                record.transition(
                    OrderPathEvent::SubmitAcceptedWithoutBrokerOrderId,
                    outcome_ts,
                )?;
                (
                    CommandAckStatus::UnknownPending,
                    Some(CommandAckReasonCode::ReconciliationRequired),
                    M3d2dLifecycleOutcomeKind::SubmittedPendingBrokerOrderIdReconciliation,
                )
            }
            FinamOrderEndpointMappedResult::Execution(FinamOrderExecutionOutcome::Rejected {
                reason_code,
            }) => {
                record.transition(OrderPathEvent::BrokerReject, outcome_ts)?;
                (
                    CommandAckStatus::Rejected,
                    Some(reason_code),
                    M3d2dLifecycleOutcomeKind::BrokerRejected,
                )
            }
            FinamOrderEndpointMappedResult::Execution(FinamOrderExecutionOutcome::Timeout) => {
                record.transition(OrderPathEvent::SubmitTimedOut, outcome_ts)?;
                (
                    CommandAckStatus::Timeout,
                    Some(CommandAckReasonCode::TransportTimeout),
                    M3d2dLifecycleOutcomeKind::TimeoutUnknownPending,
                )
            }
            other => apply_non_execution_place(&mut record, other, outcome_ts)?,
        },
        None => {
            if execution.request_sent {
                record.transition(OrderPathEvent::RequireManualIntervention, outcome_ts)?;
                record.last_ack_status = Some(CommandAckStatus::UnknownPending);
                record.last_error_kind = Some(OrderPathErrorKind::ReconciliationRequired);
                (
                    CommandAckStatus::UnknownPending,
                    Some(CommandAckReasonCode::ReconciliationRequired),
                    M3d2dLifecycleOutcomeKind::ReconciliationRequired,
                )
            } else {
                record.transition(OrderPathEvent::RequireManualIntervention, outcome_ts)?;
                record.last_ack_status = Some(CommandAckStatus::Rejected);
                record.last_error_kind = Some(OrderPathErrorKind::LocalValidation);
                (
                    CommandAckStatus::Rejected,
                    Some(CommandAckReasonCode::LocalValidationRejected),
                    M3d2dLifecycleOutcomeKind::NotSent,
                )
            }
        }
    };
    store.update_record(record.clone())?;
    Ok(report_from_record(
        M3d2dLifecycleOperation::Place,
        &record,
        outcome,
        request_sent,
        ack_status,
        reason_code,
    ))
}

pub fn m3d2d_apply_cancel_transport_execution<S>(
    store: &mut S,
    mut record: OrderPathRecord,
    approved: &PreflightApprovedCancelOrder,
    execution: M3d2RealOrderEndpointTransportExecution,
    outcome_ts: DateTime<Utc>,
) -> Result<M3d2dRealTransportLifecycleReport, M3d2dLifecycleError>
where
    S: OrderPathStore,
{
    let request_sent = execution.request_sent;
    let (ack_status, reason_code, outcome) = match execution.classified_response {
        Some(classified) => match classified.result {
            FinamOrderEndpointMappedResult::Execution(FinamOrderExecutionOutcome::Accepted {
                broker_order_id,
            }) => {
                let returned_id_mismatch = broker_order_id
                    .as_ref()
                    .is_some_and(|returned_id| returned_id != &approved.cancel().order_id);
                if returned_id_mismatch {
                    record.transition(OrderPathEvent::RequireManualIntervention, outcome_ts)?;
                    (
                        CommandAckStatus::UnknownPending,
                        Some(CommandAckReasonCode::ManualInterventionRequired),
                        M3d2dLifecycleOutcomeKind::CancelBrokerOrderIdMismatchManualIntervention,
                    )
                } else {
                    record.transition(OrderPathEvent::CancelAccepted, outcome_ts)?;
                    (
                        CommandAckStatus::Submitted,
                        Some(CommandAckReasonCode::SyntheticSubmitted),
                        M3d2dLifecycleOutcomeKind::CancelSubmittedPendingReconciliation,
                    )
                }
            }
            FinamOrderEndpointMappedResult::Execution(FinamOrderExecutionOutcome::Rejected {
                reason_code,
            }) => {
                record.transition(OrderPathEvent::CancelRejected, outcome_ts)?;
                (
                    CommandAckStatus::Rejected,
                    Some(reason_code),
                    M3d2dLifecycleOutcomeKind::CancelRejectedManualIntervention,
                )
            }
            FinamOrderEndpointMappedResult::Execution(FinamOrderExecutionOutcome::Timeout) => {
                record.transition(OrderPathEvent::CancelTimedOut, outcome_ts)?;
                (
                    CommandAckStatus::Timeout,
                    Some(CommandAckReasonCode::TransportTimeout),
                    M3d2dLifecycleOutcomeKind::CancelTimeoutUnknownPending,
                )
            }
            other => apply_non_execution_cancel(&mut record, other, outcome_ts)?,
        },
        None => {
            if execution.request_sent {
                record.transition(OrderPathEvent::RequireManualIntervention, outcome_ts)?;
                record.last_ack_status = Some(CommandAckStatus::UnknownPending);
                record.last_error_kind = Some(OrderPathErrorKind::ReconciliationRequired);
                (
                    CommandAckStatus::UnknownPending,
                    Some(CommandAckReasonCode::ReconciliationRequired),
                    M3d2dLifecycleOutcomeKind::ReconciliationRequired,
                )
            } else {
                record.transition(OrderPathEvent::RequireManualIntervention, outcome_ts)?;
                record.last_ack_status = Some(CommandAckStatus::Rejected);
                record.last_error_kind = Some(OrderPathErrorKind::LocalValidation);
                (
                    CommandAckStatus::Rejected,
                    Some(CommandAckReasonCode::LocalValidationRejected),
                    M3d2dLifecycleOutcomeKind::NotSent,
                )
            }
        }
    };
    store.update_record(record.clone())?;
    Ok(report_from_record(
        M3d2dLifecycleOperation::Cancel,
        &record,
        outcome,
        request_sent,
        ack_status,
        reason_code,
    ))
}

fn apply_non_execution_place(
    record: &mut OrderPathRecord,
    result: FinamOrderEndpointMappedResult,
    outcome_ts: DateTime<Utc>,
) -> Result<
    (
        CommandAckStatus,
        Option<CommandAckReasonCode>,
        M3d2dLifecycleOutcomeKind,
    ),
    M3d2dLifecycleError,
> {
    let (status, reason, error_kind, outcome) = non_execution_policy(result);
    record.transition(OrderPathEvent::RequireManualIntervention, outcome_ts)?;
    record.last_ack_status = Some(status);
    record.last_error_kind = Some(error_kind);
    Ok((status, Some(reason), outcome))
}

fn apply_non_execution_cancel(
    record: &mut OrderPathRecord,
    result: FinamOrderEndpointMappedResult,
    outcome_ts: DateTime<Utc>,
) -> Result<
    (
        CommandAckStatus,
        Option<CommandAckReasonCode>,
        M3d2dLifecycleOutcomeKind,
    ),
    M3d2dLifecycleError,
> {
    let (status, reason, error_kind, outcome) = non_execution_policy(result);
    record.transition(OrderPathEvent::RequireManualIntervention, outcome_ts)?;
    record.last_ack_status = Some(status);
    record.last_error_kind = Some(error_kind);
    Ok((status, Some(reason), outcome))
}

fn non_execution_policy(
    result: FinamOrderEndpointMappedResult,
) -> (
    CommandAckStatus,
    CommandAckReasonCode,
    OrderPathErrorKind,
    M3d2dLifecycleOutcomeKind,
) {
    match result {
        FinamOrderEndpointMappedResult::RateLimited { .. } => (
            CommandAckStatus::Error,
            CommandAckReasonCode::RateLimited,
            OrderPathErrorKind::RateLimited,
            M3d2dLifecycleOutcomeKind::RateLimitedDisarm,
        ),
        FinamOrderEndpointMappedResult::Maintenance { .. } => (
            CommandAckStatus::Error,
            CommandAckReasonCode::BrokerMaintenance,
            OrderPathErrorKind::BrokerMaintenance,
            M3d2dLifecycleOutcomeKind::MaintenanceDisarm,
        ),
        FinamOrderEndpointMappedResult::Unauthorized { .. } => (
            CommandAckStatus::Error,
            CommandAckReasonCode::Unauthorized,
            OrderPathErrorKind::Unauthorized,
            M3d2dLifecycleOutcomeKind::UnauthorizedDisarm,
        ),
        FinamOrderEndpointMappedResult::ReconciliationRequired { .. }
        | FinamOrderEndpointMappedResult::DecodeError { .. } => (
            CommandAckStatus::UnknownPending,
            CommandAckReasonCode::ReconciliationRequired,
            OrderPathErrorKind::ReconciliationRequired,
            M3d2dLifecycleOutcomeKind::ReconciliationRequired,
        ),
        FinamOrderEndpointMappedResult::Execution(_) => unreachable!("execution handled earlier"),
    }
}

fn report_from_record(
    operation: M3d2dLifecycleOperation,
    record: &OrderPathRecord,
    outcome: M3d2dLifecycleOutcomeKind,
    request_sent: bool,
    ack_status: CommandAckStatus,
    reason_code: Option<CommandAckReasonCode>,
) -> M3d2dRealTransportLifecycleReport {
    let _ack = record.synthetic_ack(
        ack_status,
        reason_code.map(CommandAckReason::new),
        record.last_update_ts,
    );
    let reconciliation_scheduled = matches!(
        outcome,
        M3d2dLifecycleOutcomeKind::SubmittedPendingBrokerOrderIdReconciliation
            | M3d2dLifecycleOutcomeKind::CancelSubmittedPendingReconciliation
            | M3d2dLifecycleOutcomeKind::CancelBrokerOrderIdMismatchManualIntervention
            | M3d2dLifecycleOutcomeKind::CancelTimeoutUnknownPending
            | M3d2dLifecycleOutcomeKind::TimeoutUnknownPending
            | M3d2dLifecycleOutcomeKind::ReconciliationRequired
    );
    M3d2dRealTransportLifecycleReport {
        operation,
        state: record.state,
        outcome,
        request_sent,
        submit_attempt_count: record.submit_attempt_count,
        cancel_attempt_count: record.cancel_attempt_count,
        ack_candidate: M3d2dAckCandidateDiagnostic {
            status: ack_status,
            reason_code,
            client_order_id_present: true,
            client_order_id_len: Some(record.client_order_id.as_str().len()),
            broker_order_id_present: record.broker_order_id.is_some(),
            broker_order_id_len: record
                .broker_order_id
                .as_ref()
                .map(|order_id| order_id.as_str().len()),
            raw_client_order_id_exported: false,
            raw_broker_order_id_exported: false,
            raw_account_id_exported: false,
        },
        reconciliation_scheduled,
        durable_intent_present_before_send: true,
        durable_begin_or_cancel_persisted_before_send: true,
        durable_transition_audit_required: true,
        raw_token_exported: false,
        raw_path_exported: false,
        raw_body_exported: false,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum M3d2dLifecycleError {
    #[error("missing order-path record for request {request_id}")]
    MissingOrderPathRecord { request_id: StrategyRequestId },
    #[error("missing cancel mapping for request {request_id}")]
    MissingCancelMapping { request_id: StrategyRequestId },
    #[error("order-path store error: {0}")]
    Store(#[from] OrderPathStoreError),
    #[error("order-path transition error: {0}")]
    Transition(#[from] OrderPathTransitionError),
    #[error("FINAM order request build error: {0}")]
    Build(#[from] FinamOrderRequestBuildError),
    #[error("M3d-2c transport error: {0:?}")]
    Transport(M3d2RealOrderEndpointTransportError),
}

#[allow(dead_code)]
fn _assert_no_live_runtime_surface(_signal: OperatorDisarmSignal) {}

#[cfg(test)]
mod tests {
    use super::*;
    use broker_core::{
        AccountId, BrokerOrderId, CancelOrder, CancelPreflightApproval, ClientOrderId, Exchange,
        InstrumentId, Market, OperatorArm, OrderPathEvent, OrderPathRecord, OrderPathState,
        OrderPathStore, OrderPreflightContext, OrderPreflightPolicy, OrderReferencePrice,
        OrderSide, OrderType, PlaceOrder, SqliteOrderPathReadStore, SqliteOrderPathStore,
        TimeInForce,
    };
    use chrono::TimeZone;
    use rust_decimal::Decimal;
    use serde_json::Value;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;
    use uuid::Uuid;

    use crate::m3d2_real_order_transport::{
        M3d2RealOrderEndpointTransport, M3d2RealOrderEndpointTransportConfig,
    };

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct CapturedRequest {
        method: String,
        path: String,
        authorization_is_bearer: bool,
        body_shape_ok: bool,
    }

    fn request_id(n: u128) -> StrategyRequestId {
        StrategyRequestId::from(Uuid::from_u128(n))
    }

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "TESTFUT".to_string(),
            venue_symbol: Some("TESTFUT@TEST".to_string()),
            exchange: Exchange::Other("TEST".to_string()),
            market: Market::Futures,
        }
    }

    fn place_order_with_request(n: u128) -> PlaceOrder {
        PlaceOrder {
            request_id: request_id(n),
            created_ts: Utc
                .with_ymd_and_hms(2026, 7, 3, 9, 20, 0)
                .single()
                .expect("timestamp"),
            ttl_ms: Some(1_000),
            account_id: AccountId::new("ACC_TEST_0001"),
            client_order_id: ClientOrderId::new(format!("CID{n:017}")).expect("client id"),
            instrument: instrument(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            qty: Decimal::new(1, 0),
            limit_price: Some(Decimal::new(10_050, 2)),
            time_in_force: TimeInForce::Day,
            comment: None,
        }
    }

    fn sample_arm(now: DateTime<Utc>) -> OperatorArm {
        OperatorArm {
            session_id: "ARM_TEST_M3D2D".to_string(),
            armed_until: now + chrono::Duration::minutes(5),
            endpoint_calls_enabled: true,
            one_shot: false,
            endpoint_attempted: false,
            preflight_digest: "digest-test".to_string(),
        }
    }

    fn preflight_policy(now: DateTime<Utc>) -> OrderPreflightPolicy {
        OrderPreflightPolicy {
            allowed_accounts: vec![AccountId::new("ACC_TEST_0001")],
            allowed_venue_symbols: vec!["TESTFUT@TEST".to_string()],
            allowed_order_types: vec![OrderType::Market, OrderType::Limit],
            allowed_time_in_force: vec![TimeInForce::Day],
            min_qty: Decimal::new(1, 0),
            qty_step: Decimal::new(1, 0),
            max_qty: Decimal::new(3, 0),
            price_step: Some(Decimal::new(1, 2)),
            max_market_qty: Decimal::new(1, 0),
            max_notional_per_order: None,
            max_notional_per_run: None,
            max_limit_deviation_bps: None,
            max_reference_age_ms: 1_000,
            allow_cancel_by_broker_order_id_without_mapping: false,
            operator_arm: sample_arm(now),
        }
    }

    fn approve_place(order: &PlaceOrder) -> PreflightApprovedPlaceOrder {
        let now = order.created_ts + chrono::Duration::milliseconds(1);
        let context = OrderPreflightContext {
            reference_price: Some(OrderReferencePrice {
                price: order.limit_price.unwrap_or(Decimal::new(10_050, 2)),
                received_ts: now,
            }),
            current_run_notional: Decimal::ZERO,
        };
        preflight_policy(now)
            .approve_place_order_with_context(order, now, &context)
            .expect("place preflight")
    }

    fn approve_cancel(
        cancel: &CancelOrder,
        existing: &OrderPathRecord,
    ) -> PreflightApprovedCancelOrder {
        let now = cancel.created_ts + chrono::Duration::milliseconds(1);
        match preflight_policy(now)
            .approve_cancel_order(cancel, now, Some(existing))
            .expect("cancel preflight")
        {
            CancelPreflightApproval::Submit(approved) => approved,
            CancelPreflightApproval::AlreadyTerminal => panic!("expected submit approval"),
        }
    }

    fn sqlite_path(label: &str) -> PathBuf {
        let suffix = Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_default()
            .unsigned_abs();
        std::env::temp_dir()
            .join("moex-trading-project-m3d2d")
            .join(format!("{label}-{suffix}.sqlite"))
    }

    fn cleanup_sqlite(path: &PathBuf) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(format!("{}-wal", path.to_string_lossy()));
        let _ = std::fs::remove_file(format!("{}-shm", path.to_string_lossy()));
        let mut lock = path.clone();
        lock.set_extension("writer.lock");
        let _ = std::fs::remove_file(lock);
    }

    fn insert_intent(store: &mut SqliteOrderPathStore, order: &PlaceOrder) -> OrderPathRecord {
        let record = OrderPathRecord::from_place_order(order, order.created_ts, None);
        store.insert_intent(record.clone()).expect("insert intent");
        record
    }

    fn submitted_record(
        store: &mut SqliteOrderPathStore,
        order: &PlaceOrder,
        broker_order_id: &str,
    ) -> OrderPathRecord {
        let mut record = insert_intent(store, order);
        record
            .transition(
                OrderPathEvent::BeginSubmit,
                order.created_ts + chrono::Duration::milliseconds(2),
            )
            .expect("begin");
        record.broker_order_id = Some(BrokerOrderId::new(broker_order_id));
        record
            .transition(
                OrderPathEvent::SubmitAccepted,
                order.created_ts + chrono::Duration::milliseconds(3),
            )
            .expect("submitted");
        store
            .update_record(record.clone())
            .expect("update submitted");
        record
    }

    fn response(status: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
    }

    fn run_mock_server_once(response: String) -> (String, Arc<Mutex<Option<CapturedRequest>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr");
        let captured = Arc::new(Mutex::new(None));
        let captured_server = Arc::clone(&captured);
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("timeout");
            let raw = read_http_request(&mut stream);
            *captured_server.lock().expect("lock") = Some(capture_request(&raw));
            stream.write_all(response.as_bytes()).expect("response");
        });
        (format!("http://{address}"), captured)
    }

    fn read_http_request(stream: &mut TcpStream) -> String {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let read = stream.read(&mut chunk).expect("read");
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
            let text = String::from_utf8_lossy(&buffer);
            if let Some((head, body)) = text.split_once("\r\n\r\n") {
                let content_len = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .or_else(|| {
                        head.lines()
                            .find_map(|line| line.strip_prefix("Content-Length:"))
                    })
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if body.len() >= content_len {
                    break;
                }
            }
        }
        String::from_utf8(buffer).expect("utf8 request")
    }

    fn capture_request(raw: &str) -> CapturedRequest {
        let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((raw, ""));
        let mut lines = head.lines();
        let request_line = lines.next().unwrap_or_default();
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or_default().to_string();
        let path = parts.next().unwrap_or_default().to_string();
        let authorization = head
            .lines()
            .find_map(|line| line.strip_prefix("authorization: "))
            .or_else(|| {
                head.lines()
                    .find_map(|line| line.strip_prefix("Authorization: "))
            });
        let parsed_body = serde_json::from_str::<Value>(body).ok();
        CapturedRequest {
            method,
            path,
            authorization_is_bearer: authorization
                .is_some_and(|value| value.starts_with("Bearer ") && value.len() > "Bearer ".len()),
            body_shape_ok: parsed_body.is_none_or(|value| {
                value.get("client_order_id").is_some()
                    || value.as_object().is_some_and(serde_json::Map::is_empty)
            }),
        }
    }

    fn transport(base_url: String) -> M3d2RealOrderEndpointTransport {
        M3d2RealOrderEndpointTransport::try_new(M3d2RealOrderEndpointTransportConfig {
            rest_base_url: base_url,
            ..M3d2RealOrderEndpointTransportConfig::default()
        })
        .expect("transport")
    }

    #[tokio::test]
    async fn m3d2d_place_lifecycle_persists_begin_submit_before_real_transport_send() {
        let path = sqlite_path("place-submitted");
        let (base_url, captured) = run_mock_server_once(response(
            "200 OK",
            "{\"broker_order_id\":\"BROKER_TEST_M3D2D_PLACE\"}",
        ));
        let mut store = SqliteOrderPathStore::open(&path).expect("open sqlite store");
        let order = place_order_with_request(201);
        insert_intent(&mut store, &order);
        let approved = approve_place(&order);
        let gate = EndpointGateApproved::m3d2c_test_only_for_loopback_transport();
        let token = AccessToken::new("JWT_TEST_TOKEN");

        let report = m3d2d_place_via_real_transport(
            &mut store,
            &transport(base_url),
            &gate,
            &token,
            &approved,
            None,
            M3d2dLifecycleTimestamps {
                begin_ts: order.created_ts + chrono::Duration::milliseconds(5),
                outcome_ts: order.created_ts + chrono::Duration::milliseconds(6),
            },
        )
        .await
        .expect("lifecycle report");

        assert_eq!(report.state, OrderPathState::Submitted);
        assert_eq!(report.outcome, M3d2dLifecycleOutcomeKind::Submitted);
        assert!(report.request_sent);
        assert!(!report.reconciliation_scheduled);
        assert_eq!(report.ack_candidate.status, CommandAckStatus::Submitted);
        assert!(!report.raw_token_exported);
        assert!(!report.raw_path_exported);
        assert!(!report.raw_body_exported);
        let captured = captured
            .lock()
            .expect("captured lock")
            .clone()
            .expect("captured");
        assert_eq!(captured.method, "POST");
        assert_eq!(captured.path, "/v1/accounts/ACC_TEST_0001/orders");
        assert!(captured.authorization_is_bearer);
        assert!(captured.body_shape_ok);
        let audit = store.transition_audit().expect("audit");
        assert_eq!(audit[0].event, "InsertIntent");
        assert_eq!(audit[1].event, "BeginSubmit");
        assert_eq!(audit[2].event, "SubmitAccepted");
        let serialized = serde_json::to_string(&report).expect("serialize report");
        assert!(!serialized.contains("JWT_TEST_TOKEN"));
        assert!(!serialized.contains("CID000000000000201"));
        assert!(!serialized.contains("BROKER_TEST_M3D2D_PLACE"));
        drop(store);
        cleanup_sqlite(&path);
    }

    #[tokio::test]
    async fn m3d2d_place_ambiguous_missing_id_schedules_reconciliation() {
        let path = sqlite_path("place-ambiguous");
        let (base_url, _) = run_mock_server_once(response("202 Accepted", "{}"));
        let mut store = SqliteOrderPathStore::open(&path).expect("open sqlite store");
        let order = place_order_with_request(202);
        insert_intent(&mut store, &order);
        let approved = approve_place(&order);
        let gate = EndpointGateApproved::m3d2c_test_only_for_loopback_transport();
        let token = AccessToken::new("JWT_TEST_TOKEN");

        let report = m3d2d_place_via_real_transport(
            &mut store,
            &transport(base_url),
            &gate,
            &token,
            &approved,
            None,
            M3d2dLifecycleTimestamps {
                begin_ts: order.created_ts + chrono::Duration::milliseconds(5),
                outcome_ts: order.created_ts + chrono::Duration::milliseconds(6),
            },
        )
        .await
        .expect("lifecycle report");

        assert_eq!(report.state, OrderPathState::SubmittedPendingBrokerOrderId);
        assert_eq!(
            report.outcome,
            M3d2dLifecycleOutcomeKind::SubmittedPendingBrokerOrderIdReconciliation
        );
        assert!(report.reconciliation_scheduled);
        assert_eq!(
            report.ack_candidate.status,
            CommandAckStatus::UnknownPending
        );
        assert_eq!(
            report.ack_candidate.reason_code,
            Some(CommandAckReasonCode::ReconciliationRequired)
        );
        drop(store);
        cleanup_sqlite(&path);
    }

    #[tokio::test]
    async fn m3d2d_cancel_lifecycle_persists_request_cancel_and_reconciles_conflict() {
        let path = sqlite_path("cancel-conflict");
        let (base_url, captured) =
            run_mock_server_once(response("409 Conflict", "{\"error\":\"conflict\"}"));
        let mut store = SqliteOrderPathStore::open(&path).expect("open sqlite store");
        let order = place_order_with_request(203);
        let existing = submitted_record(&mut store, &order, "BROKER_TEST_M3D2D_CANCEL");
        let cancel = CancelOrder {
            request_id: request_id(1203),
            created_ts: order.created_ts + chrono::Duration::seconds(1),
            ttl_ms: Some(1_000),
            account_id: AccountId::new("ACC_TEST_0001"),
            order_id: BrokerOrderId::new("BROKER_TEST_M3D2D_CANCEL"),
            client_order_id: Some(order.client_order_id.clone()),
        };
        let approved = approve_cancel(&cancel, &existing);
        let gate = EndpointGateApproved::m3d2c_test_only_for_loopback_transport();
        let token = AccessToken::new("JWT_TEST_TOKEN");

        let report = m3d2d_cancel_via_real_transport(
            &mut store,
            &transport(base_url),
            &gate,
            &token,
            &approved,
            M3d2dLifecycleTimestamps {
                begin_ts: cancel.created_ts + chrono::Duration::milliseconds(5),
                outcome_ts: cancel.created_ts + chrono::Duration::milliseconds(6),
            },
        )
        .await
        .expect("cancel lifecycle report");

        assert_eq!(report.state, OrderPathState::ManualInterventionRequired);
        assert_eq!(
            report.outcome,
            M3d2dLifecycleOutcomeKind::ReconciliationRequired
        );
        assert!(report.reconciliation_scheduled);
        assert_eq!(
            report.ack_candidate.status,
            CommandAckStatus::UnknownPending
        );
        assert_eq!(
            report.ack_candidate.reason_code,
            Some(CommandAckReasonCode::ReconciliationRequired)
        );
        let captured = captured
            .lock()
            .expect("captured lock")
            .clone()
            .expect("captured");
        assert_eq!(captured.method, "DELETE");
        assert_eq!(
            captured.path,
            "/v1/accounts/ACC_TEST_0001/orders/BROKER_TEST_M3D2D_CANCEL"
        );
        let audit = store.transition_audit().expect("audit");
        assert_eq!(
            audit.last().expect("audit last").event,
            "RequireManualIntervention"
        );
        drop(store);
        cleanup_sqlite(&path);
    }

    #[tokio::test]
    async fn m3d2d_crash_windows_recover_conservatively_without_blind_retry() {
        let before_send_path = sqlite_path("crash-before-send");
        {
            let mut store = SqliteOrderPathStore::open(&before_send_path).expect("open");
            let order = place_order_with_request(204);
            insert_intent(&mut store, &order);
        }
        {
            let readonly =
                SqliteOrderPathReadStore::open_readonly(&before_send_path).expect("readonly");
            let loaded = readonly
                .operator_load_by_request_id(request_id(204))
                .expect("loaded");
            assert_eq!(loaded.state, OrderPathState::IntentRecorded);
        }
        cleanup_sqlite(&before_send_path);

        let after_begin_path = sqlite_path("crash-after-begin");
        {
            let mut store = SqliteOrderPathStore::open(&after_begin_path).expect("open");
            let order = place_order_with_request(205);
            insert_intent(&mut store, &order);
            let approved = approve_place(&order);
            let _ = m3d2d_persist_place_begin_submit(
                &mut store,
                &approved,
                order.created_ts + chrono::Duration::milliseconds(5),
            )
            .expect("begin persisted");
        }
        {
            let mut store = SqliteOrderPathStore::open(&after_begin_path).expect("reopen");
            let mut loaded = store
                .load_by_request_id(request_id(205))
                .expect("loaded after begin");
            assert_eq!(loaded.state, OrderPathState::SubmitInFlight);
            loaded
                .recover_after_restart(loaded.last_update_ts + chrono::Duration::seconds(1))
                .expect("recover");
            store
                .update_record(loaded.clone())
                .expect("persist recovery");
            assert_eq!(loaded.state, OrderPathState::TimeoutUnknownPending);
        }
        cleanup_sqlite(&after_begin_path);

        let after_send_path = sqlite_path("crash-after-send");
        {
            let (base_url, captured) = run_mock_server_once(response(
                "200 OK",
                "{\"broker_order_id\":\"BROKER_TEST_M3D2D_AFTER_SEND\"}",
            ));
            let mut store = SqliteOrderPathStore::open(&after_send_path).expect("open");
            let order = place_order_with_request(206);
            insert_intent(&mut store, &order);
            let approved = approve_place(&order);
            let record = m3d2d_persist_place_begin_submit(
                &mut store,
                &approved,
                order.created_ts + chrono::Duration::milliseconds(5),
            )
            .expect("begin persisted");
            assert_eq!(record.state, OrderPathState::SubmitInFlight);
            let spec = broker_finam::build_place_order_request(&approved, None).expect("spec");
            let gate = EndpointGateApproved::m3d2c_test_only_for_loopback_transport();
            let token = AccessToken::new("JWT_TEST_TOKEN");
            let execution = transport(base_url)
                .place_order_execution(&gate, &token, &spec)
                .await;
            assert!(execution.request_sent);
            assert!(captured.lock().expect("captured").is_some());
        }
        {
            let mut store = SqliteOrderPathStore::open(&after_send_path).expect("reopen");
            let mut loaded = store
                .load_by_request_id(request_id(206))
                .expect("loaded after send");
            assert_eq!(loaded.state, OrderPathState::SubmitInFlight);
            loaded
                .recover_after_restart(loaded.last_update_ts + chrono::Duration::seconds(1))
                .expect("recover");
            store
                .update_record(loaded.clone())
                .expect("persist recovery");
            assert_eq!(loaded.state, OrderPathState::TimeoutUnknownPending);
        }
        cleanup_sqlite(&after_send_path);

        let after_classified_path = sqlite_path("crash-after-classified");
        {
            let (base_url, _) = run_mock_server_once(response(
                "200 OK",
                "{\"broker_order_id\":\"BROKER_TEST_M3D2D_CLASSIFIED\"}",
            ));
            let mut store = SqliteOrderPathStore::open(&after_classified_path).expect("open");
            let order = place_order_with_request(207);
            insert_intent(&mut store, &order);
            let approved = approve_place(&order);
            let gate = EndpointGateApproved::m3d2c_test_only_for_loopback_transport();
            let token = AccessToken::new("JWT_TEST_TOKEN");
            let report = m3d2d_place_via_real_transport(
                &mut store,
                &transport(base_url),
                &gate,
                &token,
                &approved,
                None,
                M3d2dLifecycleTimestamps {
                    begin_ts: order.created_ts + chrono::Duration::milliseconds(5),
                    outcome_ts: order.created_ts + chrono::Duration::milliseconds(6),
                },
            )
            .await
            .expect("lifecycle report");
            assert_eq!(report.state, OrderPathState::Submitted);
        }
        {
            let readonly =
                SqliteOrderPathReadStore::open_readonly(&after_classified_path).expect("readonly");
            let loaded = readonly
                .operator_load_by_request_id(request_id(207))
                .expect("loaded after classified");
            assert_eq!(loaded.state, OrderPathState::Submitted);
        }
        cleanup_sqlite(&after_classified_path);
    }
}
