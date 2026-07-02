//! Design-only API shape for a future gateway-owned FINAM real order endpoint
//! boundary.
//!
//! This module intentionally does not perform route rendering from live inputs,
//! does not own a network connector, and does not submit FINAM order requests.
//! It only records the pre-implementation boundary shape that a later reviewed
//! implementation must satisfy. Any path template kept here is internal-only;
//! exported diagnostics are redacted.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

use broker_core::command::{CommandAckReasonCode, CommandAckStatus};
use broker_core::{OperatorDisarmSignal, OrderPathErrorKind, OrderPathEvent, OrderPathState};

use crate::{
    EndpointGateApproved, M3cOrderEndpointNegativeTestPlanItem,
    M3cOrderEndpointScannerTransitionMode, RuntimeCommandAckIdPolicy,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointBoundaryMode {
    DesignOnlyNoHttpSend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointOperation {
    PlaceOrder,
    CancelOrder,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct GatewayRealOrderEndpointInternalRouteShape {
    pub operation: GatewayRealOrderEndpointOperation,
    pub method_name: &'static str,
    pub route_template: &'static str,
    pub gate_marker_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointRedactedRouteDiagnostic {
    pub operation: GatewayRealOrderEndpointOperation,
    pub method_name: String,
    pub route_template_redacted: bool,
    pub route_template_exported: bool,
    pub gate_marker_required: bool,
}

struct RenderedOrderEndpointPath(String);

enum ApprovedOrderEndpointRequestSpec {
    Place(broker_finam::FinamPlaceOrderRequestSpec),
    Cancel(broker_finam::FinamCancelOrderRequestSpec),
}

struct OrderEndpointAccountInstrumentAllowlistApproved {
    pub account_allowlisted: bool,
    pub instrument_allowlisted: bool,
}

struct OrderEndpointOperatorArmApproved {
    pub operator_arm_validated: bool,
    pub one_shot_arm: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointDurableCheckpointLabel {
    PlaceBeginSubmitPersistedBeforeEndpoint,
    CancelRequestCancelPersistedBeforeEndpoint,
}

// Design-only marker for a future implementation. It must become constructible
// only after the Place BeginSubmit SQLite transition is durably persisted.
#[allow(dead_code)]
struct PlaceEndpointDurableCheckpointApproved {
    _private: (),
}

// Design-only marker for a future implementation. It must become constructible
// only after the Cancel RequestCancel SQLite transition is durably persisted.
#[allow(dead_code)]
struct CancelEndpointDurableCheckpointApproved {
    _private: (),
}

struct GatewayRealOrderEndpointRequestSnapshotFingerprint {
    pub operation: GatewayRealOrderEndpointOperation,
    pub request_id_hash_present: bool,
    pub client_order_id_hash_present: bool,
    pub account_hash_present: bool,
    pub instrument_hash_present: bool,
    pub fingerprint_sha256_len: usize,
    pub raw_values_exported: bool,
    pub matches_approved_request_parts: bool,
}

#[allow(dead_code)]
struct GatewayRealOrderEndpointAttemptFingerprint {
    pub endpoint_attempt_id_hash_present: bool,
    pub endpoint_attempt_id_sha256_len: usize,
    pub request_snapshot_fingerprint_present: bool,
    pub raw_attempt_id_exported: bool,
    pub raw_request_values_exported: bool,
}

struct GatewayRealOrderEndpointSqliteTransitionCommitProof {
    pub event: OrderPathEvent,
    pub durable_commit_observed: bool,
    pub diagnostic_or_report_source: bool,
    pub request_snapshot_fingerprint: GatewayRealOrderEndpointRequestSnapshotFingerprint,
    pub marker_single_use: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GatewayRealOrderEndpointCheckpointMarkerCreationError {
    WrongTransitionEvent,
    DurableCommitNotObserved,
    DiagnosticOrReportLayer,
    RequestSnapshotFingerprintMissing,
    RequestSnapshotFingerprintMismatch,
    RawRequestIdentityExported,
    MarkerAlreadyUsed,
}

#[allow(dead_code)]
struct GatewayRealOrderEndpointCapturedEnvelopeRecord {
    pub kind: GatewayRealOrderEndpointCapturedEnvelopeKind,
    pub status_code_present: bool,
    pub body_presence_len_hash_only: bool,
    pub transport_category: Option<GatewayRealOrderEndpointTransportCategory>,
    pub raw_path_exported: bool,
    pub raw_body_exported: bool,
    pub raw_error_exported: bool,
}

#[allow(dead_code)]
struct GatewayRealOrderEndpointAttemptJournalBinding {
    pub attempt_fingerprint: GatewayRealOrderEndpointAttemptFingerprint,
    pub request_parts_bound: bool,
    pub checkpoint_marker_bound: bool,
    pub captured_envelope_bound: bool,
    pub outcome_classifier_bound: bool,
    pub state_machine_transition_required: bool,
}

#[allow(dead_code)]
struct GatewayRealOrderEndpointCheckpointProofFingerprint {
    pub checkpoint_proof_fingerprint_present: bool,
    pub checkpoint_proof_sha256_len: usize,
    pub raw_checkpoint_values_exported: bool,
}

#[allow(dead_code)]
struct GatewayRealOrderEndpointCapturedEnvelopeFingerprint {
    pub captured_envelope_fingerprint_present: bool,
    pub captured_envelope_sha256_len: usize,
    pub raw_path_body_error_exported: bool,
}

#[allow(dead_code)]
struct GatewayRealOrderEndpointOutcomeClassifierFingerprint {
    pub outcome_fingerprint_present: bool,
    pub outcome_sha256_len: usize,
    pub raw_broker_values_exported: bool,
}

#[allow(dead_code)]
struct GatewayRealOrderEndpointStateTransitionResultRecord {
    pub event: OrderPathEvent,
    pub state: OrderPathState,
    pub transition_committed: bool,
    pub transition_result_fingerprint_present: bool,
    pub transition_result_sha256_len: usize,
    pub raw_transition_values_exported: bool,
}

#[allow(dead_code)]
struct GatewayRealOrderEndpointAckDiagnosticFingerprint {
    pub ack_diagnostic_fingerprint_present: bool,
    pub ack_diagnostic_sha256_len: usize,
    pub runtime_ack_redacted_only: bool,
    pub raw_ack_id_exported: bool,
}

#[allow(dead_code)]
struct GatewayRealOrderEndpointDurableAttemptJournalAppendInput {
    pub parts: ApprovedOrderEndpointRequestParts,
    pub attempt_fingerprint: GatewayRealOrderEndpointAttemptFingerprint,
    pub checkpoint_proof_fingerprint: GatewayRealOrderEndpointCheckpointProofFingerprint,
    pub captured_envelope_fingerprint: GatewayRealOrderEndpointCapturedEnvelopeFingerprint,
    pub outcome_fingerprint: GatewayRealOrderEndpointOutcomeClassifierFingerprint,
    pub state_transition_result: GatewayRealOrderEndpointStateTransitionResultRecord,
    pub ack_diagnostic_fingerprint: GatewayRealOrderEndpointAckDiagnosticFingerprint,
}

#[allow(dead_code)]
struct GatewayRealOrderEndpointDurableAttemptJournalRecord {
    pub endpoint_attempt_id_hash_present: bool,
    pub request_fingerprint_bound: bool,
    pub checkpoint_proof_fingerprint_bound: bool,
    pub captured_envelope_fingerprint_bound: bool,
    pub outcome_fingerprint_bound: bool,
    pub state_transition_result_bound: bool,
    pub ack_diagnostic_fingerprint_bound: bool,
    pub append_committed_after_state_transition: bool,
    pub raw_values_exported: bool,
}

struct OrderEndpointDurableStateCheckpoint {
    pub intent_recorded_before_endpoint: bool,
    pub label: GatewayRealOrderEndpointDurableCheckpointLabel,
}

struct ApprovedOrderEndpointRequestParts {
    pub operation: GatewayRealOrderEndpointOperation,
    pub method_name: &'static str,
    pub rendered_path: RenderedOrderEndpointPath,
    pub approved_request_spec: ApprovedOrderEndpointRequestSpec,
    pub account_instrument_allowlist_approved: bool,
    pub operator_arm_approved: bool,
    pub durable_state_checkpoint_present: bool,
    pub durable_checkpoint_label: GatewayRealOrderEndpointDurableCheckpointLabel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GatewayRealOrderEndpointApprovedPartsError {
    AccountInstrumentAllowlist,
    OperatorArm,
    DurableStateCheckpoint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointApprovedPartsDesignShape {
    pub approved_request_parts_type_internal: bool,
    pub rendered_path_type_internal: bool,
    pub rendered_path_exported: bool,
    pub raw_body_exported: bool,
    pub diagnostic_can_construct_request_parts: bool,
    pub constructors_require_endpoint_gate: bool,
    pub constructors_require_approved_request_spec: bool,
    pub constructors_require_account_instrument_allowlist: bool,
    pub constructors_require_operator_arm: bool,
    pub constructors_require_durable_state_checkpoint: bool,
    pub constructors_require_operation_specific_checkpoint: bool,
    pub constructor_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointApprovedPartsDiagnostic {
    pub operation: GatewayRealOrderEndpointOperation,
    pub method_name: String,
    pub rendered_path_present: bool,
    pub rendered_path_redacted: bool,
    pub rendered_path_exported: bool,
    pub raw_body_exported: bool,
    pub account_id_present: bool,
    pub account_id_len: usize,
    pub order_id_present: bool,
    pub order_id_len: Option<usize>,
    pub symbol_present: bool,
    pub symbol_len: Option<usize>,
    pub account_instrument_allowlist_approved: bool,
    pub operator_arm_approved: bool,
    pub durable_state_checkpoint_present: bool,
    pub durable_checkpoint_label: GatewayRealOrderEndpointDurableCheckpointLabel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointConsumerDesignShape {
    pub consumer_internal_only: bool,
    pub consumer_requires_endpoint_gate: bool,
    pub consumer_accepts_approved_request_parts_only: bool,
    pub consumer_accepts_diagnostics: bool,
    pub consumer_network_enabled: bool,
    pub rendered_path_exported: bool,
    pub raw_body_exported: bool,
    pub runtime_ack_redacted_only: bool,
    pub consumer_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointConsumerDiagnostic {
    pub operation: GatewayRealOrderEndpointOperation,
    pub method_name: String,
    pub accepted_approved_request_parts: bool,
    pub endpoint_gate_required: bool,
    pub network_enabled: bool,
    pub rendered_path_present: bool,
    pub rendered_path_redacted: bool,
    pub rendered_path_exported: bool,
    pub raw_body_exported: bool,
    pub runtime_ack_redacted_only: bool,
    pub account_id_present: bool,
    pub account_id_len: usize,
    pub order_id_present: bool,
    pub order_id_len: Option<usize>,
    pub symbol_present: bool,
    pub symbol_len: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointFutureSendOutcome {
    Accepted,
    Rejected,
    TimeoutUnknownPending,
    RateLimited,
    Maintenance,
    Unauthorized,
    DecodeError,
    TransportError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointFutureSendResultDesignShape {
    pub design_only: bool,
    pub outcome_count: usize,
    pub future_send_requires_endpoint_gate: bool,
    pub future_send_accepts_approved_request_parts_only: bool,
    pub future_send_accepts_diagnostics: bool,
    pub future_send_consumes_request_parts: bool,
    pub future_send_network_enabled: bool,
    pub operation_specific_durable_checkpoint_required: bool,
    pub place_checkpoint_label: GatewayRealOrderEndpointDurableCheckpointLabel,
    pub cancel_checkpoint_label: GatewayRealOrderEndpointDurableCheckpointLabel,
    pub retry_after_timeout_unknown_allowed: bool,
    pub request_parts_reuse_after_outcome_allowed: bool,
    pub result_diagnostic_can_bypass_state_machine: bool,
    pub state_machine_transition_required: bool,
    pub rendered_path_exported: bool,
    pub raw_body_exported: bool,
    pub runtime_ack_redacted_only: bool,
    pub classifier_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointFutureSendDiagnostic {
    pub outcome: GatewayRealOrderEndpointFutureSendOutcome,
    pub operation: GatewayRealOrderEndpointOperation,
    pub method_name: String,
    pub endpoint_gate_required: bool,
    pub request_parts_consumed: bool,
    pub request_parts_reuse_after_outcome_allowed: bool,
    pub network_enabled: bool,
    pub rendered_path_present: bool,
    pub rendered_path_redacted: bool,
    pub rendered_path_exported: bool,
    pub raw_body_exported: bool,
    pub account_id_present: bool,
    pub account_id_len: usize,
    pub order_id_present: bool,
    pub order_id_len: Option<usize>,
    pub symbol_present: bool,
    pub symbol_len: Option<usize>,
    pub durable_checkpoint_label: GatewayRealOrderEndpointDurableCheckpointLabel,
    pub retry_after_timeout_unknown_allowed: bool,
    pub state_machine_transition_required: bool,
    pub state_machine_bypass_allowed: bool,
    pub runtime_ack_redacted_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointTransportCategory {
    DnsOrConnectError,
    TlsError,
    HttpSendError,
    BodyReadError,
    Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointTransportStateSemantics {
    NonTimeoutTransportFailure,
    TimeoutUnknownPending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointOperatorPolicy {
    None,
    BackoffAndManualIntervention,
    DegradeAndManualIntervention,
    DisarmAndOperatorIntervention,
    DecodeManualIntervention,
    TransportCategoryManualIntervention,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointTransportCategoryPolicyEntry {
    pub category: GatewayRealOrderEndpointTransportCategory,
    pub send_outcome: GatewayRealOrderEndpointFutureSendOutcome,
    pub state_semantics: GatewayRealOrderEndpointTransportStateSemantics,
    pub place_event: OrderPathEvent,
    pub place_state: OrderPathState,
    pub cancel_event: OrderPathEvent,
    pub cancel_state: OrderPathState,
    pub error_kind: Option<OrderPathErrorKind>,
    pub ack_status: CommandAckStatus,
    pub place_ack_reason_code: Option<CommandAckReasonCode>,
    pub cancel_ack_reason_code: Option<CommandAckReasonCode>,
    pub operator_policy: GatewayRealOrderEndpointOperatorPolicy,
    pub operator_disarm_signal: Option<OperatorDisarmSignal>,
    pub reconciliation_required: bool,
    pub backoff_required: bool,
    pub manual_intervention_required: bool,
    pub no_blind_retry: bool,
    pub timeout_unknown_pending_semantics: bool,
    pub non_timeout_transport_semantics: bool,
    pub state_machine_transition_required: bool,
    pub result_diagnostic_can_bypass_state_machine: bool,
    pub runtime_ack_redacted_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointTransportCategoryPolicyDesignShape {
    pub matrix_serializable: bool,
    pub category_count: usize,
    pub timeout_category_count: usize,
    pub non_timeout_category_count: usize,
    pub timeout_separated_from_non_timeout_transport: bool,
    pub non_timeout_transport_does_not_use_timeout_ack_reason: bool,
    pub non_timeout_transport_does_not_enter_timeout_unknown_state: bool,
    pub timeout_uses_unknown_pending_semantics: bool,
    pub diagnostic_can_bypass_state_machine: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointOutcomeStatePolicyEntry {
    pub outcome: GatewayRealOrderEndpointFutureSendOutcome,
    pub place_event: OrderPathEvent,
    pub place_state: OrderPathState,
    pub cancel_event: OrderPathEvent,
    pub cancel_state: OrderPathState,
    pub error_kind: Option<OrderPathErrorKind>,
    pub ack_status: CommandAckStatus,
    pub place_ack_reason_code: Option<CommandAckReasonCode>,
    pub cancel_ack_reason_code: Option<CommandAckReasonCode>,
    pub operator_policy: GatewayRealOrderEndpointOperatorPolicy,
    pub operator_disarm_signal: Option<OperatorDisarmSignal>,
    pub backoff_required: bool,
    pub manual_intervention_required: bool,
    pub no_blind_retry: bool,
    pub state_machine_transition_required: bool,
    pub result_diagnostic_can_bypass_state_machine: bool,
    pub runtime_ack_redacted_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointAcceptedResultKind {
    WithBrokerOrderId,
    WithoutBrokerOrderId,
    EmptyBrokerOrderId,
    BrokerOrderIdMismatch,
}

struct GatewayRealOrderEndpointAcceptedResponseShape {
    pub kind: GatewayRealOrderEndpointAcceptedResultKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointAcceptedBrokerIdPolicy {
    AcceptedWithBrokerOrderId,
    AcceptedWithoutBrokerOrderId,
    EmptyBrokerOrderIdDecodeError,
    BrokerOrderIdMismatchManualIntervention,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointAcceptedBrokerIdPolicyEntry {
    pub policy: GatewayRealOrderEndpointAcceptedBrokerIdPolicy,
    pub place_event: OrderPathEvent,
    pub place_state: OrderPathState,
    pub ack_status: CommandAckStatus,
    pub ack_reason_code: Option<CommandAckReasonCode>,
    pub operator_disarm_signal: Option<OperatorDisarmSignal>,
    pub reconciliation_required: bool,
    pub no_blind_retry: bool,
    pub manual_intervention_required: bool,
    pub raw_broker_order_id_exported: bool,
    pub runtime_ack_redacted_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointAcceptedResultPolicyEntry {
    pub kind: GatewayRealOrderEndpointAcceptedResultKind,
    pub inherited_policy: GatewayRealOrderEndpointAcceptedBrokerIdPolicy,
    pub send_outcome: GatewayRealOrderEndpointFutureSendOutcome,
    pub place_event: OrderPathEvent,
    pub place_state: OrderPathState,
    pub ack_status: CommandAckStatus,
    pub ack_reason_code: Option<CommandAckReasonCode>,
    pub operator_disarm_signal: Option<OperatorDisarmSignal>,
    pub reconciliation_required: bool,
    pub no_blind_retry: bool,
    pub manual_intervention_required: bool,
    pub raw_broker_order_id_exported: bool,
    pub runtime_ack_redacted_only: bool,
    pub state_machine_transition_required: bool,
    pub accepted_result_can_be_treated_as_unconditional_submitted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointAcceptedResultClassifierDesignShape {
    pub classifier_internal_only: bool,
    pub classifier_requires_endpoint_gate: bool,
    pub classifier_accepts_approved_request_parts_only: bool,
    pub classifier_accepts_diagnostics: bool,
    pub classifier_consumes_request_parts: bool,
    pub accepted_kind_count: usize,
    pub accepted_policy_entry_count: usize,
    pub accepted_broker_id_policy_wired: bool,
    pub raw_broker_order_id_exported: bool,
    pub unconditional_submitted_allowed: bool,
    pub state_machine_transition_required: bool,
    pub classifier_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointAcceptedResultDiagnostic {
    pub kind: GatewayRealOrderEndpointAcceptedResultKind,
    pub operation: GatewayRealOrderEndpointOperation,
    pub method_name: String,
    pub endpoint_gate_required: bool,
    pub request_parts_consumed: bool,
    pub accepted_response_shape_consumed: bool,
    pub uses_accepted_broker_id_policy_matrix: bool,
    pub raw_broker_order_id_exported: bool,
    pub broker_order_id_redacted: bool,
    pub state_machine_transition_required: bool,
    pub state_machine_bypass_allowed: bool,
    pub runtime_ack_redacted_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointCancelAcceptedIdPolicy {
    MatchingBrokerOrderId,
    MissingBrokerOrderIdAcceptedPendingReconciliation,
    BrokerOrderIdMismatchManualIntervention,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointCancelAcceptedIdPolicyEntry {
    pub policy: GatewayRealOrderEndpointCancelAcceptedIdPolicy,
    pub cancel_event: OrderPathEvent,
    pub cancel_state: OrderPathState,
    pub ack_status: CommandAckStatus,
    pub ack_reason_code: Option<CommandAckReasonCode>,
    pub operator_disarm_signal: Option<OperatorDisarmSignal>,
    pub response_body_required: bool,
    pub broker_order_id_required: bool,
    pub reconciliation_required: bool,
    pub no_blind_retry: bool,
    pub manual_intervention_required: bool,
    pub raw_broker_order_id_exported: bool,
    pub runtime_ack_redacted_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointCancelAcceptedIdPolicyDesignShape {
    pub policy_entry_count: usize,
    pub response_body_optional_documented: bool,
    pub matching_id_ok: bool,
    pub missing_id_requires_reconciliation: bool,
    pub mismatched_id_manual_conflict: bool,
    pub raw_broker_order_id_exported: bool,
    pub no_blind_retry_required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointCapturedEnvelopeKind {
    AcceptedResponse,
    BrokerErrorResponse,
    TransportError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointCapturedEnvelopeTransportCategoryEntry {
    pub category: GatewayRealOrderEndpointTransportCategory,
    pub envelope_kind: GatewayRealOrderEndpointCapturedEnvelopeKind,
    pub raw_error_exported: bool,
    pub transport_category_exported: bool,
    pub error_len_recorded: bool,
    pub error_sha256_recorded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointCapturedResponseEnvelopeDiagnostic {
    pub kind: GatewayRealOrderEndpointCapturedEnvelopeKind,
    pub operation: GatewayRealOrderEndpointOperation,
    pub method_name: String,
    pub endpoint_gate_required: bool,
    pub request_parts_consumed: bool,
    pub request_snapshot_fingerprint_present: bool,
    pub status_code_present: bool,
    pub body_present: bool,
    pub body_len_recorded: bool,
    pub body_sha256_recorded: bool,
    pub transport_category: Option<GatewayRealOrderEndpointTransportCategory>,
    pub error_len_recorded: bool,
    pub error_sha256_recorded: bool,
    pub raw_path_exported: bool,
    pub raw_body_exported: bool,
    pub raw_error_exported: bool,
    pub runtime_ack_redacted_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointCapturedEnvelopeDesignShape {
    pub envelope_diagnostic_redacted_only: bool,
    pub envelope_requires_endpoint_gate: bool,
    pub envelope_accepts_approved_request_parts_only: bool,
    pub envelope_accepts_raw_path_body_or_error: bool,
    pub raw_path_exported: bool,
    pub raw_body_exported: bool,
    pub raw_error_exported: bool,
    pub status_len_hash_presence_only: bool,
    pub transport_category_mapped: bool,
    pub transport_category_mapping_entry_count: usize,
    pub diagnostic_can_feed_transport: bool,
    pub runtime_ack_redacted_only: bool,
    pub diagnostic_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointHttpBodyShape {
    AcceptedWithBrokerOrderId,
    AcceptedWithoutBrokerOrderId,
    AcceptedEmptyBrokerOrderId,
    AcceptedBrokerOrderIdMismatch,
    BrokerReject,
    Unauthorized,
    Timeout,
    RateLimit,
    Maintenance,
    MalformedBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointHttpStatusOutcomeEntry {
    pub operation: GatewayRealOrderEndpointOperation,
    pub status_code: u16,
    pub body_shape: GatewayRealOrderEndpointHttpBodyShape,
    pub envelope_kind: GatewayRealOrderEndpointCapturedEnvelopeKind,
    pub outcome: GatewayRealOrderEndpointFutureSendOutcome,
    pub accepted_result_kind: Option<GatewayRealOrderEndpointAcceptedResultKind>,
    pub cancel_accepted_id_policy: Option<GatewayRealOrderEndpointCancelAcceptedIdPolicy>,
    pub order_path_event: OrderPathEvent,
    pub order_path_state: OrderPathState,
    pub ack_status: CommandAckStatus,
    pub ack_reason_code: Option<CommandAckReasonCode>,
    pub operator_disarm_signal: Option<OperatorDisarmSignal>,
    pub state_machine_transition_required: bool,
    pub captured_envelope_required: bool,
    pub endpoint_attempt_journal_required: bool,
    pub no_blind_retry: bool,
    pub raw_path_exported: bool,
    pub raw_body_exported: bool,
    pub raw_error_exported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointHttpStatusOutcomeMatrixDesignShape {
    pub matrix_serializable: bool,
    pub place_entry_count: usize,
    pub cancel_entry_count: usize,
    pub total_entry_count: usize,
    pub covers_2xx_accepted_body_variants: bool,
    pub covers_400_422_broker_reject: bool,
    pub covers_401_403_unauthorized: bool,
    pub covers_408_504_timeout: bool,
    pub covers_429_rate_limit: bool,
    pub covers_500_502_503_maintenance: bool,
    pub covers_malformed_body_decode_error: bool,
    pub covers_transport_category_failures: bool,
    pub place_cancel_specific_mapping: bool,
    pub raw_path_exported: bool,
    pub raw_body_exported: bool,
    pub raw_error_exported: bool,
    pub state_machine_transition_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointAttemptJournalDesignShape {
    pub journal_internal_only: bool,
    pub endpoint_attempt_id_hash_len: usize,
    pub attempt_id_raw_exported: bool,
    pub binds_approved_request_parts: bool,
    pub binds_request_snapshot_fingerprint: bool,
    pub binds_checkpoint_marker: bool,
    pub binds_captured_envelope: bool,
    pub binds_outcome_classifier: bool,
    pub raw_path_exported: bool,
    pub raw_body_exported: bool,
    pub raw_error_exported: bool,
    pub diagnostic_redacted_only: bool,
    pub diagnostic_can_feed_transport: bool,
    pub diagnostic_can_bypass_state_machine: bool,
    pub constructor_count: usize,
    pub diagnostic_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointAttemptDiagnostic {
    pub operation: GatewayRealOrderEndpointOperation,
    pub endpoint_attempt_id_hash_present: bool,
    pub endpoint_attempt_id_sha256_len: usize,
    pub request_snapshot_fingerprint_present: bool,
    pub request_parts_bound: bool,
    pub checkpoint_marker_bound: bool,
    pub captured_envelope_bound: bool,
    pub outcome_classifier_bound: bool,
    pub state_machine_transition_required: bool,
    pub state_machine_bypass_allowed: bool,
    pub raw_path_exported: bool,
    pub raw_body_exported: bool,
    pub raw_error_exported: bool,
    pub runtime_ack_redacted_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointDurableAttemptJournalContractDesignShape {
    pub durable_journal_schema_design_only: bool,
    pub journal_record_internal_only: bool,
    pub append_requires_endpoint_gate: bool,
    pub append_requires_approved_request_parts: bool,
    pub append_requires_operation_specific_checkpoint_marker: bool,
    pub endpoint_attempt_id_hash_len: usize,
    pub binds_request_fingerprint: bool,
    pub binds_checkpoint_proof_fingerprint: bool,
    pub binds_captured_envelope_fingerprint: bool,
    pub binds_outcome_fingerprint: bool,
    pub binds_state_transition_result_fingerprint: bool,
    pub binds_ack_diagnostic_fingerprint: bool,
    pub append_committed_after_state_transition: bool,
    pub raw_endpoint_attempt_id_exported: bool,
    pub raw_request_values_exported: bool,
    pub raw_broker_order_id_exported: bool,
    pub raw_path_exported: bool,
    pub raw_body_exported: bool,
    pub raw_error_exported: bool,
    pub diagnostic_redacted_only: bool,
    pub diagnostic_can_feed_transport: bool,
    pub diagnostic_can_bypass_state_machine: bool,
    pub exact_once_attempt_id_unique_required: bool,
    pub replay_requires_same_fingerprint_set: bool,
    pub append_constructor_count: usize,
    pub diagnostic_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointDurableAttemptJournalDiagnostic {
    pub operation: GatewayRealOrderEndpointOperation,
    pub endpoint_attempt_id_hash_present: bool,
    pub endpoint_attempt_id_sha256_len: usize,
    pub request_fingerprint_bound: bool,
    pub checkpoint_proof_fingerprint_bound: bool,
    pub captured_envelope_fingerprint_bound: bool,
    pub outcome_fingerprint_bound: bool,
    pub state_transition_result_fingerprint_bound: bool,
    pub ack_diagnostic_fingerprint_bound: bool,
    pub append_committed_after_state_transition: bool,
    pub state_machine_transition_required: bool,
    pub state_machine_bypass_allowed: bool,
    pub raw_endpoint_attempt_id_exported: bool,
    pub raw_request_values_exported: bool,
    pub raw_broker_order_id_exported: bool,
    pub raw_path_exported: bool,
    pub raw_body_exported: bool,
    pub raw_error_exported: bool,
    pub runtime_ack_redacted_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointFinamStatusBodyPolicy {
    PlaceSuccessBodyRequiredForSubmitted,
    CancelSuccessBodyOptional,
    Undocumented2xxRequiresEvidenceOrWaiver,
    BrokerRejectBodyRedacted,
    UnauthorizedBodyRedacted,
    NotFoundRequiresReadOnlyReconciliation,
    RateLimitBodyRedacted,
    MaintenanceBodyRedacted,
    TimeoutUnknownPending,
    MalformedBodyDecodeError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointFinamStatusSemanticsEntry {
    pub operation: GatewayRealOrderEndpointOperation,
    pub status_code: u16,
    pub documented_by_finam_rest_docs: bool,
    pub defensive_policy_only: bool,
    pub implementation_gate_evidence_required: bool,
    pub waiver_required_before_live: bool,
    pub body_policy: GatewayRealOrderEndpointFinamStatusBodyPolicy,
    pub future_send_outcome: GatewayRealOrderEndpointFutureSendOutcome,
    pub order_path_event: OrderPathEvent,
    pub order_path_state: OrderPathState,
    pub ack_status: CommandAckStatus,
    pub ack_reason_code: Option<CommandAckReasonCode>,
    pub operator_disarm_signal: Option<OperatorDisarmSignal>,
    pub body_required_for_immediate_submitted: bool,
    pub body_optional_for_cancel_acceptance: bool,
    pub empty_body_reconciliation_required: bool,
    pub cancel_reconciliation_required: bool,
    pub no_blind_retry: bool,
    pub raw_path_exported: bool,
    pub raw_body_exported: bool,
    pub raw_error_exported: bool,
    pub raw_broker_order_id_exported: bool,
    pub state_machine_transition_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointFinamStatusSemanticsDesignShape {
    pub official_rest_docs_checked: bool,
    pub documented_place_status_count: usize,
    pub documented_cancel_status_count: usize,
    pub place_status_entry_count: usize,
    pub cancel_status_entry_count: usize,
    pub total_status_entry_count: usize,
    pub documented_success_status_only_200: bool,
    pub undocumented_success_201_202_204_require_evidence_or_waiver: bool,
    pub place_success_body_required_for_immediate_submitted: bool,
    pub place_empty_body_requires_reconciliation: bool,
    pub cancel_success_body_optional: bool,
    pub cancel_missing_id_requires_reconciliation: bool,
    pub cancel_404_documented_and_requires_reconciliation: bool,
    pub cancel_409_410_documented_by_finam_rest_docs: bool,
    pub cancel_409_410_policy_or_waiver_required: bool,
    pub defensive_422_502_not_documented_as_finam_order_status: bool,
    pub status_semantics_can_bypass_state_machine: bool,
    pub raw_path_exported: bool,
    pub raw_body_exported: bool,
    pub raw_error_exported: bool,
    pub raw_broker_order_id_exported: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointEvidenceSlot {
    ReleaseProfileEvidenceOrWaiver,
    PositiveGetOrderEvidenceOrWaiver,
    RouteTemplateRecheck,
    Undocumented2xxStatusSemantics,
    Cancel409410StatusSemantics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointEvidenceClosureMethod {
    ControlledEvidence,
    ReviewerAcceptedWaiver,
    OfficialDocsConfirmation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointEvidenceClosureStatus {
    PendingBeforeImplementationGate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointEvidenceSlotClosurePlanEntry {
    pub slot: GatewayRealOrderEndpointEvidenceSlot,
    pub current_status: GatewayRealOrderEndpointEvidenceClosureStatus,
    pub accepted_closure_methods: Vec<GatewayRealOrderEndpointEvidenceClosureMethod>,
    pub must_close_before_implementation_gate: bool,
    pub endpoint_calls_allowed_for_closure: bool,
    pub order_endpoint_calls_allowed_for_closure: bool,
    pub reviewer_acceptance_required: bool,
    pub artifact_redacted_only: bool,
    pub source_archive_binding_required: bool,
    pub raw_secret_exported: bool,
    pub raw_account_exported: bool,
    pub raw_order_id_exported: bool,
    pub raw_path_exported: bool,
    pub raw_body_exported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointEvidenceClosurePlanDesignShape {
    pub plan_design_only: bool,
    pub slot_count: usize,
    pub release_profile_slot_present: bool,
    pub positive_get_order_slot_present: bool,
    pub route_template_recheck_slot_present: bool,
    pub undocumented_2xx_policy_slot_present: bool,
    pub cancel_409_410_policy_slot_present: bool,
    pub all_slots_pending_before_implementation_gate: bool,
    pub all_slots_require_reviewer_acceptance: bool,
    pub order_endpoint_calls_allowed_for_closure: bool,
    pub closure_artifacts_redacted_only: bool,
    pub source_archive_binding_required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointDurableAttemptJournalColumnKind {
    PrimaryKeyHash,
    ForeignKeyHash,
    SafeEnum,
    Timestamp,
    Sha256,
    Boolean,
    Integer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointDurableAttemptJournalColumn {
    pub name: String,
    pub kind: GatewayRealOrderEndpointDurableAttemptJournalColumnKind,
    pub required: bool,
    pub unique: bool,
    pub stores_raw_value: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointDurableAttemptJournalIndex {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub replay_policy_related: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointDurableAttemptJournalReplayDecision {
    SameFingerprintSetIdempotentReplay,
    DifferentFingerprintSetRejectAndDisarm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointDurableAttemptJournalReplayPolicyEntry {
    pub decision: GatewayRealOrderEndpointDurableAttemptJournalReplayDecision,
    pub endpoint_attempt_id_hash_match_required: bool,
    pub full_fingerprint_set_match_required: bool,
    pub state_transition_result_match_required: bool,
    pub ack_diagnostic_match_required: bool,
    pub raw_values_compared: bool,
    pub no_blind_retry: bool,
    pub operator_disarm_on_conflict: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointDurableAttemptJournalSqliteSchemaDesignShape {
    pub schema_design_only: bool,
    pub table_name: String,
    pub schema_version: u16,
    pub column_count: usize,
    pub unique_index_count: usize,
    pub replay_policy_entry_count: usize,
    pub endpoint_attempt_id_unique: bool,
    pub request_id_hash_indexed: bool,
    pub client_order_id_hash_indexed: bool,
    pub broker_order_id_hash_optional: bool,
    pub order_path_record_reference_required: bool,
    pub append_only: bool,
    pub begin_immediate_required: bool,
    pub wal_required: bool,
    pub synchronous_full_required: bool,
    pub writer_lock_required: bool,
    pub schema_version_guard_required: bool,
    pub idempotent_replay_requires_same_fingerprint_set: bool,
    pub conflict_replay_rejects_and_disarms: bool,
    pub raw_endpoint_attempt_id_exported: bool,
    pub raw_request_values_exported: bool,
    pub raw_broker_order_id_exported: bool,
    pub raw_path_exported: bool,
    pub raw_body_exported: bool,
    pub raw_error_exported: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointDurableJournalMigrationStepKind {
    BackupBeforeMigration,
    AcquireSingleWriterLock,
    OpenSqliteWithWal,
    SetSynchronousFull,
    VerifySchemaVersion,
    CreateOrderEndpointAttemptsTable,
    CreateReplayIndexes,
    RunIntegrityCheck,
    RefuseAutoRepair,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointDurableJournalMigrationStep {
    pub step: GatewayRealOrderEndpointDurableJournalMigrationStepKind,
    pub required_before_endpoint_gate: bool,
    pub operator_visible: bool,
    pub failure_disarms_order_endpoints: bool,
    pub raw_values_exported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointDurableJournalMigrationRunbookDesignShape {
    pub migration_runbook_design_only: bool,
    pub step_count: usize,
    pub backup_required_before_migration: bool,
    pub begin_immediate_required_for_schema_change: bool,
    pub wal_required: bool,
    pub synchronous_full_required: bool,
    pub single_writer_lock_required: bool,
    pub schema_version_guard_required: bool,
    pub sqlite_integrity_check_required: bool,
    pub corruption_open_failure_disarms: bool,
    pub stale_or_unknown_lock_disarms: bool,
    pub auto_repair_allowed: bool,
    pub auto_stale_lock_delete_allowed: bool,
    pub operator_runbook_required: bool,
    pub redacted_operator_diagnostics_only: bool,
    pub raw_sqlite_path_exported: bool,
    pub raw_request_values_exported: bool,
    pub raw_broker_payload_exported: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointCanonicalReplayFingerprintField {
    SchemaVersion,
    Operation,
    EndpointAttemptIdSha256,
    RequestIdSha256,
    ClientOrderIdSha256,
    AccountSha256,
    InstrumentSha256,
    CheckpointLabel,
    RequestFingerprintSha256,
    CheckpointProofSha256,
    CapturedEnvelopeSha256,
    OutcomeSha256,
    StateTransitionSha256,
    AckDiagnosticSha256,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointCanonicalReplayEncoding {
    Utf8JsonObjectSortedKeysNoWhitespace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointCanonicalReplayFingerprintFieldEntry {
    pub ordinal: usize,
    pub field: GatewayRealOrderEndpointCanonicalReplayFingerprintField,
    pub required: bool,
    pub hash_len: usize,
    pub raw_value_exported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointCanonicalReplayFingerprintDesignShape {
    pub fingerprint_spec_design_only: bool,
    pub field_count: usize,
    pub encoding: GatewayRealOrderEndpointCanonicalReplayEncoding,
    pub schema_version_included: bool,
    pub operation_included: bool,
    pub endpoint_attempt_id_included: bool,
    pub request_client_account_instrument_hashes_included: bool,
    pub checkpoint_and_envelope_hashes_included: bool,
    pub outcome_state_ack_hashes_included: bool,
    pub stable_field_order_required: bool,
    pub sorted_keys_required: bool,
    pub whitespace_forbidden: bool,
    pub sha256_len: usize,
    pub raw_values_exported: bool,
    pub refactor_changes_require_schema_bump: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointAttemptIdLifecyclePhase {
    GeneratedAfterApprovedRequestParts,
    BoundBeforeEndpointSend,
    PersistedWithAttemptJournal,
    ReusedOnlyForIdempotentReplay,
    NeverReusedForNewAttemptAfterTerminalOrManual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointAttemptIdLifecyclePolicyEntry {
    pub phase: GatewayRealOrderEndpointAttemptIdLifecyclePhase,
    pub requires_endpoint_gate: bool,
    pub requires_request_id_hash: bool,
    pub requires_client_order_id_hash: bool,
    pub requires_operation: bool,
    pub endpoint_attempt_id_sha256_len: usize,
    pub new_network_attempt_allowed: bool,
    pub raw_endpoint_attempt_id_exported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointAttemptIdLifecycleDesignShape {
    pub lifecycle_design_only: bool,
    pub phase_count: usize,
    pub generated_after_approved_request_parts: bool,
    pub generated_before_future_endpoint_send: bool,
    pub bound_to_request_and_client_id_hashes: bool,
    pub bound_to_operation: bool,
    pub persisted_before_outcome_export: bool,
    pub same_attempt_id_replay_requires_same_fingerprint_set: bool,
    pub reuse_after_timeout_manual_or_terminal_allowed: bool,
    pub new_endpoint_attempt_requires_new_id: bool,
    pub raw_endpoint_attempt_id_exported: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointImplementationGateReadinessItem {
    ForbiddenSurfaceScanners,
    EndpointGateUnconstructible,
    DurableStoreImplementedTested,
    OperatorArmImplementedTested,
    RateLimitBackoffImplementedTested,
    NoBlindRetryImplementedTested,
    RedactedAckImplementedTested,
    ReleaseProfileEvidenceOrWaiver,
    PositiveGetOrderEvidenceOrWaiver,
    RouteTemplateRecheck,
    CanonicalReplayGoldenVectors,
    OperatorReplayRunbook,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointImplementationGateReadinessStatus {
    ImplementedAndTested,
    DesignRecorded,
    PendingEvidenceOrWaiver,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointImplementationGateReadinessChecklistEntry {
    pub item: GatewayRealOrderEndpointImplementationGateReadinessItem,
    pub status: GatewayRealOrderEndpointImplementationGateReadinessStatus,
    pub must_close_before_implementation_gate: bool,
    pub reviewer_acceptance_required: bool,
    pub endpoint_calls_allowed_for_check: bool,
    pub raw_values_exported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointImplementationGateReadinessDesignShape {
    pub checklist_design_only: bool,
    pub checklist_entry_count: usize,
    pub implemented_tested_count: usize,
    pub pending_evidence_or_waiver_count: usize,
    pub release_profile_evidence_or_waiver_pending: bool,
    pub positive_get_order_evidence_or_waiver_pending: bool,
    pub route_template_recheck_pending: bool,
    pub canonical_replay_golden_vectors_present: bool,
    pub operator_replay_runbook_present: bool,
    pub endpoint_calls_allowed_for_readiness: bool,
    pub raw_values_exported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointCanonicalReplayGoldenVector {
    pub name: String,
    pub schema_version: u16,
    pub encoding: GatewayRealOrderEndpointCanonicalReplayEncoding,
    pub canonical_json: String,
    pub expected_sha256: String,
    pub field_count: usize,
    pub no_whitespace: bool,
    pub sorted_keys_required: bool,
    pub raw_values_exported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointCanonicalReplayGoldenVectorDesignShape {
    pub golden_vectors_design_only: bool,
    pub vector_count: usize,
    pub canonical_json_no_whitespace: bool,
    pub expected_sha256_len: usize,
    pub all_fields_hash_or_safe_label: bool,
    pub raw_values_exported: bool,
    pub refactor_changes_require_vector_update_and_schema_bump: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointOperatorReplayRunbookCase {
    IdempotentReplaySameFingerprint,
    ConflictingReplayDisarm,
    TimeoutUnknownPending,
    ManualIntervention,
    TerminalOutcomeNewAttempt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointOperatorReplayRunbookEntry {
    pub case: GatewayRealOrderEndpointOperatorReplayRunbookCase,
    pub operator_visible: bool,
    pub redacted_diagnostic_only: bool,
    pub same_endpoint_attempt_id_allowed: bool,
    pub new_endpoint_attempt_id_required: bool,
    pub disarm_required: bool,
    pub no_blind_retry: bool,
    pub raw_endpoint_attempt_id_exported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointOperatorReplayRunbookDesignShape {
    pub operator_runbook_design_only: bool,
    pub case_count: usize,
    pub idempotent_replay_case_present: bool,
    pub conflicting_replay_disarms: bool,
    pub timeout_requires_new_attempt_id: bool,
    pub manual_requires_new_attempt_id: bool,
    pub terminal_requires_new_attempt_id: bool,
    pub redacted_diagnostics_only: bool,
    pub raw_endpoint_attempt_id_exported: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayRealOrderEndpointEvidenceClosurePackageStatus {
    PendingEvidenceOrWaiver,
    EvidenceProvided,
    WaiverAccepted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointEvidenceClosurePackageEntry {
    pub slot: GatewayRealOrderEndpointEvidenceSlot,
    pub status: GatewayRealOrderEndpointEvidenceClosurePackageStatus,
    pub evidence_or_waiver_required: bool,
    pub reviewer_acceptance_required: bool,
    pub source_archive_binding_required: bool,
    pub order_endpoint_calls_allowed_for_closure: bool,
    pub raw_values_exported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointEvidenceClosurePackageDesignShape {
    pub closure_package_design_only: bool,
    pub slot_count: usize,
    pub release_profile_slot_present: bool,
    pub positive_get_order_slot_present: bool,
    pub route_template_recheck_slot_present: bool,
    pub undocumented_2xx_slot_present: bool,
    pub cancel_409_410_slot_present: bool,
    pub all_slots_require_evidence_or_waiver: bool,
    pub all_slots_require_reviewer_acceptance: bool,
    pub source_archive_binding_required: bool,
    pub order_endpoint_calls_allowed_for_closure: bool,
    pub raw_values_exported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointRouteTemplateRecheckPlanDesignShape {
    pub route_template_recheck_design_only: bool,
    pub route_count: usize,
    pub exact_two_route_allowlist_required: bool,
    pub official_docs_or_waiver_required: bool,
    pub reviewer_acceptance_required: bool,
    pub recheck_before_implementation_gate: bool,
    pub route_templates_exported_as_design_data_only: bool,
    pub rendered_routes_exported: bool,
    pub raw_account_or_order_id_exported: bool,
    pub order_endpoint_calls_allowed_for_recheck: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointEvidenceReportReadinessDesignShape {
    pub evidence_report_readiness_design_only: bool,
    pub canonical_replay_golden_vector_sha256: String,
    pub canonical_replay_vector_count: usize,
    pub readiness_implemented_tested_count: usize,
    pub readiness_pending_evidence_or_waiver_count: usize,
    pub operator_replay_runbook_case_count: usize,
    pub raw_values_exported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointOutcomeStatePolicyDesignShape {
    pub matrix_serializable: bool,
    pub outcome_entry_count: usize,
    pub accepted_broker_id_policy_entry_count: usize,
    pub ack_reason_mapping_redacted: bool,
    pub operator_disarm_backoff_manual_matrix_present: bool,
    pub accepted_broker_id_policy_inherited: bool,
    pub timeout_no_blind_retry_invariant: bool,
    pub outcome_diagnostic_can_bypass_state_machine: bool,
    pub state_machine_transition_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointDurableCheckpointCapabilityDesignShape {
    pub place_capability_type_internal: bool,
    pub cancel_capability_type_internal: bool,
    pub capability_not_debug_or_serializable: bool,
    pub created_after_sqlite_transition_only: bool,
    pub place_required_event: OrderPathEvent,
    pub place_required_label: GatewayRealOrderEndpointDurableCheckpointLabel,
    pub cancel_required_event: OrderPathEvent,
    pub cancel_required_label: GatewayRealOrderEndpointDurableCheckpointLabel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointCheckpointMarkerCreationDesignShape {
    pub place_marker_creation_function_private: bool,
    pub cancel_marker_creation_function_private: bool,
    pub creation_requires_endpoint_gate: bool,
    pub creation_requires_sqlite_transition_commit_proof: bool,
    pub proof_bound_to_request_snapshot_fingerprint: bool,
    pub proof_fingerprint_includes_request_client_account_instrument_hashes: bool,
    pub proof_raw_request_values_exported: bool,
    pub marker_single_use_required: bool,
    pub checkpoint_reuse_across_intents_allowed: bool,
    pub creation_rejects_diagnostic_or_report_source: bool,
    pub creation_requires_durable_commit_observed: bool,
    pub creation_requires_transition_event_match: bool,
    pub creation_requires_fingerprint_operation_match: bool,
    pub place_required_event: OrderPathEvent,
    pub cancel_required_event: OrderPathEvent,
    pub marker_not_debug_or_serializable: bool,
    pub constructor_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointApiShape {
    pub mode: GatewayRealOrderEndpointBoundaryMode,
    pub approved_module_path: String,
    pub route_rendering_requires_gate_marker: bool,
    pub http_send_requires_gate_marker: bool,
    pub api_shape_contains_route_templates: bool,
    pub approved_request_parts_design: GatewayRealOrderEndpointApprovedPartsDesignShape,
    pub consumer_design: GatewayRealOrderEndpointConsumerDesignShape,
    pub future_send_result_design: GatewayRealOrderEndpointFutureSendResultDesignShape,
    pub transport_category_policy_design:
        GatewayRealOrderEndpointTransportCategoryPolicyDesignShape,
    pub outcome_state_policy_design: GatewayRealOrderEndpointOutcomeStatePolicyDesignShape,
    pub accepted_result_classifier_design:
        GatewayRealOrderEndpointAcceptedResultClassifierDesignShape,
    pub cancel_accepted_id_policy_design: GatewayRealOrderEndpointCancelAcceptedIdPolicyDesignShape,
    pub captured_envelope_design: GatewayRealOrderEndpointCapturedEnvelopeDesignShape,
    pub endpoint_attempt_journal_design: GatewayRealOrderEndpointAttemptJournalDesignShape,
    pub durable_attempt_journal_contract_design:
        GatewayRealOrderEndpointDurableAttemptJournalContractDesignShape,
    pub http_status_outcome_matrix_design:
        GatewayRealOrderEndpointHttpStatusOutcomeMatrixDesignShape,
    pub finam_status_semantics_design: GatewayRealOrderEndpointFinamStatusSemanticsDesignShape,
    pub implementation_gate_evidence_closure_plan_design:
        GatewayRealOrderEndpointEvidenceClosurePlanDesignShape,
    pub durable_attempt_journal_sqlite_schema_design:
        GatewayRealOrderEndpointDurableAttemptJournalSqliteSchemaDesignShape,
    pub durable_journal_migration_runbook_design:
        GatewayRealOrderEndpointDurableJournalMigrationRunbookDesignShape,
    pub canonical_replay_fingerprint_design:
        GatewayRealOrderEndpointCanonicalReplayFingerprintDesignShape,
    pub endpoint_attempt_id_lifecycle_design: GatewayRealOrderEndpointAttemptIdLifecycleDesignShape,
    pub implementation_gate_readiness_design:
        GatewayRealOrderEndpointImplementationGateReadinessDesignShape,
    pub canonical_replay_golden_vector_design:
        GatewayRealOrderEndpointCanonicalReplayGoldenVectorDesignShape,
    pub operator_replay_runbook_design: GatewayRealOrderEndpointOperatorReplayRunbookDesignShape,
    pub evidence_closure_package_design: GatewayRealOrderEndpointEvidenceClosurePackageDesignShape,
    pub route_template_recheck_plan_design:
        GatewayRealOrderEndpointRouteTemplateRecheckPlanDesignShape,
    pub evidence_report_readiness_design:
        GatewayRealOrderEndpointEvidenceReportReadinessDesignShape,
    pub durable_checkpoint_capability_design:
        GatewayRealOrderEndpointDurableCheckpointCapabilityDesignShape,
    pub checkpoint_marker_creation_design:
        GatewayRealOrderEndpointCheckpointMarkerCreationDesignShape,
    pub runtime_ack_id_policy: RuntimeCommandAckIdPolicy,
    pub scanner_transition_spec: GatewayRealOrderEndpointScannerTransitionSpec,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointScannerTransitionSpec {
    pub current_mode: M3cOrderEndpointScannerTransitionMode,
    pub future_mode: M3cOrderEndpointScannerTransitionMode,
    pub exact_place_order_surface_count: usize,
    pub exact_cancel_order_surface_count: usize,
    pub approved_module_path: String,
    pub allowed_route_template_count: usize,
    pub negative_tests: Vec<M3cOrderEndpointNegativeTestPlanItem>,
    pub real_post_delete_calls_allowed_now: bool,
}

pub fn api_shape() -> GatewayRealOrderEndpointApiShape {
    let approved_module_path = "crates/finam-gateway/src/real_order_endpoint.rs".to_string();
    GatewayRealOrderEndpointApiShape {
        mode: GatewayRealOrderEndpointBoundaryMode::DesignOnlyNoHttpSend,
        approved_module_path: approved_module_path.clone(),
        route_rendering_requires_gate_marker: true,
        http_send_requires_gate_marker: true,
        api_shape_contains_route_templates: false,
        approved_request_parts_design: GatewayRealOrderEndpointApprovedPartsDesignShape {
            approved_request_parts_type_internal: true,
            rendered_path_type_internal: true,
            rendered_path_exported: false,
            raw_body_exported: false,
            diagnostic_can_construct_request_parts: false,
            constructors_require_endpoint_gate: true,
            constructors_require_approved_request_spec: true,
            constructors_require_account_instrument_allowlist: true,
            constructors_require_operator_arm: true,
            constructors_require_durable_state_checkpoint: true,
            constructors_require_operation_specific_checkpoint: true,
            constructor_count: approved_request_parts_constructor_count(),
        },
        consumer_design: GatewayRealOrderEndpointConsumerDesignShape {
            consumer_internal_only: true,
            consumer_requires_endpoint_gate: true,
            consumer_accepts_approved_request_parts_only: true,
            consumer_accepts_diagnostics: false,
            consumer_network_enabled: false,
            rendered_path_exported: false,
            raw_body_exported: false,
            runtime_ack_redacted_only: true,
            consumer_count: approved_request_parts_consumer_count(),
        },
        future_send_result_design: GatewayRealOrderEndpointFutureSendResultDesignShape {
            design_only: true,
            outcome_count: future_send_outcome_count(),
            future_send_requires_endpoint_gate: true,
            future_send_accepts_approved_request_parts_only: true,
            future_send_accepts_diagnostics: false,
            future_send_consumes_request_parts: true,
            future_send_network_enabled: false,
            operation_specific_durable_checkpoint_required: true,
            place_checkpoint_label:
                GatewayRealOrderEndpointDurableCheckpointLabel::PlaceBeginSubmitPersistedBeforeEndpoint,
            cancel_checkpoint_label:
                GatewayRealOrderEndpointDurableCheckpointLabel::CancelRequestCancelPersistedBeforeEndpoint,
            retry_after_timeout_unknown_allowed: false,
            request_parts_reuse_after_outcome_allowed: false,
            result_diagnostic_can_bypass_state_machine: false,
            state_machine_transition_required: true,
            rendered_path_exported: false,
            raw_body_exported: false,
            runtime_ack_redacted_only: true,
            classifier_count: future_send_result_classifier_count(),
        },
        transport_category_policy_design:
            GatewayRealOrderEndpointTransportCategoryPolicyDesignShape {
                matrix_serializable: true,
                category_count: transport_category_count(),
                timeout_category_count: 1,
                non_timeout_category_count: 4,
                timeout_separated_from_non_timeout_transport: true,
                non_timeout_transport_does_not_use_timeout_ack_reason: true,
                non_timeout_transport_does_not_enter_timeout_unknown_state: true,
                timeout_uses_unknown_pending_semantics: true,
                diagnostic_can_bypass_state_machine: false,
            },
        outcome_state_policy_design: GatewayRealOrderEndpointOutcomeStatePolicyDesignShape {
            matrix_serializable: true,
            outcome_entry_count: future_send_outcome_state_policy_matrix().len(),
            accepted_broker_id_policy_entry_count: accepted_broker_id_policy_matrix().len(),
            ack_reason_mapping_redacted: true,
            operator_disarm_backoff_manual_matrix_present: true,
            accepted_broker_id_policy_inherited: true,
            timeout_no_blind_retry_invariant: true,
            outcome_diagnostic_can_bypass_state_machine: false,
            state_machine_transition_required: true,
        },
        accepted_result_classifier_design:
            GatewayRealOrderEndpointAcceptedResultClassifierDesignShape {
                classifier_internal_only: true,
                classifier_requires_endpoint_gate: true,
                classifier_accepts_approved_request_parts_only: true,
                classifier_accepts_diagnostics: false,
                classifier_consumes_request_parts: true,
                accepted_kind_count: accepted_result_kind_count(),
                accepted_policy_entry_count: accepted_result_classifier_policy_matrix().len(),
                accepted_broker_id_policy_wired: true,
                raw_broker_order_id_exported: false,
                unconditional_submitted_allowed: false,
                state_machine_transition_required: true,
                classifier_count: accepted_result_classifier_count(),
            },
        cancel_accepted_id_policy_design:
            GatewayRealOrderEndpointCancelAcceptedIdPolicyDesignShape {
                policy_entry_count: cancel_accepted_id_policy_matrix().len(),
                response_body_optional_documented: true,
                matching_id_ok: true,
                missing_id_requires_reconciliation: true,
                mismatched_id_manual_conflict: true,
                raw_broker_order_id_exported: false,
                no_blind_retry_required: true,
            },
        captured_envelope_design: GatewayRealOrderEndpointCapturedEnvelopeDesignShape {
            envelope_diagnostic_redacted_only: true,
            envelope_requires_endpoint_gate: true,
            envelope_accepts_approved_request_parts_only: true,
            envelope_accepts_raw_path_body_or_error: false,
            raw_path_exported: false,
            raw_body_exported: false,
            raw_error_exported: false,
            status_len_hash_presence_only: true,
            transport_category_mapped: true,
            transport_category_mapping_entry_count: captured_envelope_transport_category_matrix()
                .len(),
            diagnostic_can_feed_transport: false,
            runtime_ack_redacted_only: true,
            diagnostic_count: captured_envelope_diagnostic_count(),
        },
        endpoint_attempt_journal_design: GatewayRealOrderEndpointAttemptJournalDesignShape {
            journal_internal_only: true,
            endpoint_attempt_id_hash_len: 64,
            attempt_id_raw_exported: false,
            binds_approved_request_parts: true,
            binds_request_snapshot_fingerprint: true,
            binds_checkpoint_marker: true,
            binds_captured_envelope: true,
            binds_outcome_classifier: true,
            raw_path_exported: false,
            raw_body_exported: false,
            raw_error_exported: false,
            diagnostic_redacted_only: true,
            diagnostic_can_feed_transport: false,
            diagnostic_can_bypass_state_machine: false,
            constructor_count: endpoint_attempt_journal_binding_constructor_count(),
            diagnostic_count: endpoint_attempt_diagnostic_count(),
        },
        durable_attempt_journal_contract_design:
            GatewayRealOrderEndpointDurableAttemptJournalContractDesignShape {
                durable_journal_schema_design_only: true,
                journal_record_internal_only: true,
                append_requires_endpoint_gate: true,
                append_requires_approved_request_parts: true,
                append_requires_operation_specific_checkpoint_marker: true,
                endpoint_attempt_id_hash_len: 64,
                binds_request_fingerprint: true,
                binds_checkpoint_proof_fingerprint: true,
                binds_captured_envelope_fingerprint: true,
                binds_outcome_fingerprint: true,
                binds_state_transition_result_fingerprint: true,
                binds_ack_diagnostic_fingerprint: true,
                append_committed_after_state_transition: true,
                raw_endpoint_attempt_id_exported: false,
                raw_request_values_exported: false,
                raw_broker_order_id_exported: false,
                raw_path_exported: false,
                raw_body_exported: false,
                raw_error_exported: false,
                diagnostic_redacted_only: true,
                diagnostic_can_feed_transport: false,
                diagnostic_can_bypass_state_machine: false,
                exact_once_attempt_id_unique_required: true,
                replay_requires_same_fingerprint_set: true,
                append_constructor_count: durable_attempt_journal_append_constructor_count(),
                diagnostic_count: durable_attempt_journal_diagnostic_count(),
            },
        http_status_outcome_matrix_design:
            GatewayRealOrderEndpointHttpStatusOutcomeMatrixDesignShape {
                matrix_serializable: true,
                place_entry_count: place_http_status_outcome_matrix().len(),
                cancel_entry_count: cancel_http_status_outcome_matrix().len(),
                total_entry_count: http_status_outcome_matrix().len(),
                covers_2xx_accepted_body_variants: true,
                covers_400_422_broker_reject: true,
                covers_401_403_unauthorized: true,
                covers_408_504_timeout: true,
                covers_429_rate_limit: true,
                covers_500_502_503_maintenance: true,
                covers_malformed_body_decode_error: true,
                covers_transport_category_failures: true,
                place_cancel_specific_mapping: true,
                raw_path_exported: false,
                raw_body_exported: false,
                raw_error_exported: false,
                state_machine_transition_required: true,
            },
        finam_status_semantics_design: GatewayRealOrderEndpointFinamStatusSemanticsDesignShape {
            official_rest_docs_checked: true,
            documented_place_status_count: documented_place_finam_rest_statuses().len(),
            documented_cancel_status_count: documented_cancel_finam_rest_statuses().len(),
            place_status_entry_count: place_finam_status_semantics_matrix().len(),
            cancel_status_entry_count: cancel_finam_status_semantics_matrix().len(),
            total_status_entry_count: finam_status_semantics_matrix().len(),
            documented_success_status_only_200: true,
            undocumented_success_201_202_204_require_evidence_or_waiver: true,
            place_success_body_required_for_immediate_submitted: true,
            place_empty_body_requires_reconciliation: true,
            cancel_success_body_optional: true,
            cancel_missing_id_requires_reconciliation: true,
            cancel_404_documented_and_requires_reconciliation: true,
            cancel_409_410_documented_by_finam_rest_docs: false,
            cancel_409_410_policy_or_waiver_required: true,
            defensive_422_502_not_documented_as_finam_order_status: true,
            status_semantics_can_bypass_state_machine: false,
            raw_path_exported: false,
            raw_body_exported: false,
            raw_error_exported: false,
            raw_broker_order_id_exported: false,
        },
        implementation_gate_evidence_closure_plan_design:
            GatewayRealOrderEndpointEvidenceClosurePlanDesignShape {
                plan_design_only: true,
                slot_count: implementation_gate_evidence_closure_plan().len(),
                release_profile_slot_present: true,
                positive_get_order_slot_present: true,
                route_template_recheck_slot_present: true,
                undocumented_2xx_policy_slot_present: true,
                cancel_409_410_policy_slot_present: true,
                all_slots_pending_before_implementation_gate: true,
                all_slots_require_reviewer_acceptance: true,
                order_endpoint_calls_allowed_for_closure: false,
                closure_artifacts_redacted_only: true,
                source_archive_binding_required: true,
            },
        durable_attempt_journal_sqlite_schema_design:
            GatewayRealOrderEndpointDurableAttemptJournalSqliteSchemaDesignShape {
                schema_design_only: true,
                table_name: "order_endpoint_attempts".to_string(),
                schema_version: 1,
                column_count: durable_attempt_journal_sqlite_columns().len(),
                unique_index_count: durable_attempt_journal_sqlite_indexes()
                    .iter()
                    .filter(|index| index.unique)
                    .count(),
                replay_policy_entry_count: durable_attempt_journal_replay_policy_matrix().len(),
                endpoint_attempt_id_unique: true,
                request_id_hash_indexed: true,
                client_order_id_hash_indexed: true,
                broker_order_id_hash_optional: true,
                order_path_record_reference_required: true,
                append_only: true,
                begin_immediate_required: true,
                wal_required: true,
                synchronous_full_required: true,
                writer_lock_required: true,
                schema_version_guard_required: true,
                idempotent_replay_requires_same_fingerprint_set: true,
                conflict_replay_rejects_and_disarms: true,
                raw_endpoint_attempt_id_exported: false,
                raw_request_values_exported: false,
                raw_broker_order_id_exported: false,
                raw_path_exported: false,
                raw_body_exported: false,
                raw_error_exported: false,
            },
        durable_journal_migration_runbook_design:
            GatewayRealOrderEndpointDurableJournalMigrationRunbookDesignShape {
                migration_runbook_design_only: true,
                step_count: durable_journal_migration_runbook_steps().len(),
                backup_required_before_migration: true,
                begin_immediate_required_for_schema_change: true,
                wal_required: true,
                synchronous_full_required: true,
                single_writer_lock_required: true,
                schema_version_guard_required: true,
                sqlite_integrity_check_required: true,
                corruption_open_failure_disarms: true,
                stale_or_unknown_lock_disarms: true,
                auto_repair_allowed: false,
                auto_stale_lock_delete_allowed: false,
                operator_runbook_required: true,
                redacted_operator_diagnostics_only: true,
                raw_sqlite_path_exported: false,
                raw_request_values_exported: false,
                raw_broker_payload_exported: false,
            },
        canonical_replay_fingerprint_design:
            GatewayRealOrderEndpointCanonicalReplayFingerprintDesignShape {
                fingerprint_spec_design_only: true,
                field_count: canonical_replay_fingerprint_fields().len(),
                encoding:
                    GatewayRealOrderEndpointCanonicalReplayEncoding::Utf8JsonObjectSortedKeysNoWhitespace,
                schema_version_included: true,
                operation_included: true,
                endpoint_attempt_id_included: true,
                request_client_account_instrument_hashes_included: true,
                checkpoint_and_envelope_hashes_included: true,
                outcome_state_ack_hashes_included: true,
                stable_field_order_required: true,
                sorted_keys_required: true,
                whitespace_forbidden: true,
                sha256_len: 64,
                raw_values_exported: false,
                refactor_changes_require_schema_bump: true,
            },
        endpoint_attempt_id_lifecycle_design:
            GatewayRealOrderEndpointAttemptIdLifecycleDesignShape {
                lifecycle_design_only: true,
                phase_count: endpoint_attempt_id_lifecycle_policy().len(),
                generated_after_approved_request_parts: true,
                generated_before_future_endpoint_send: true,
                bound_to_request_and_client_id_hashes: true,
                bound_to_operation: true,
                persisted_before_outcome_export: true,
                same_attempt_id_replay_requires_same_fingerprint_set: true,
                reuse_after_timeout_manual_or_terminal_allowed: false,
                new_endpoint_attempt_requires_new_id: true,
                raw_endpoint_attempt_id_exported: false,
            },
        implementation_gate_readiness_design:
            GatewayRealOrderEndpointImplementationGateReadinessDesignShape {
                checklist_design_only: true,
                checklist_entry_count: implementation_gate_readiness_checklist().len(),
                implemented_tested_count: implementation_gate_readiness_checklist()
                    .iter()
                    .filter(|entry| {
                        entry.status
                            == GatewayRealOrderEndpointImplementationGateReadinessStatus::ImplementedAndTested
                    })
                    .count(),
                pending_evidence_or_waiver_count: implementation_gate_readiness_checklist()
                    .iter()
                    .filter(|entry| {
                        entry.status
                            == GatewayRealOrderEndpointImplementationGateReadinessStatus::PendingEvidenceOrWaiver
                    })
                    .count(),
                release_profile_evidence_or_waiver_pending: true,
                positive_get_order_evidence_or_waiver_pending: true,
                route_template_recheck_pending: true,
                canonical_replay_golden_vectors_present: true,
                operator_replay_runbook_present: true,
                endpoint_calls_allowed_for_readiness: false,
                raw_values_exported: false,
            },
        canonical_replay_golden_vector_design:
            GatewayRealOrderEndpointCanonicalReplayGoldenVectorDesignShape {
                golden_vectors_design_only: true,
                vector_count: canonical_replay_golden_vectors().len(),
                canonical_json_no_whitespace: true,
                expected_sha256_len: 64,
                all_fields_hash_or_safe_label: true,
                raw_values_exported: false,
                refactor_changes_require_vector_update_and_schema_bump: true,
            },
        operator_replay_runbook_design: GatewayRealOrderEndpointOperatorReplayRunbookDesignShape {
            operator_runbook_design_only: true,
            case_count: operator_replay_runbook_entries().len(),
            idempotent_replay_case_present: true,
            conflicting_replay_disarms: true,
            timeout_requires_new_attempt_id: true,
            manual_requires_new_attempt_id: true,
            terminal_requires_new_attempt_id: true,
            redacted_diagnostics_only: true,
            raw_endpoint_attempt_id_exported: false,
        },
        evidence_closure_package_design:
            GatewayRealOrderEndpointEvidenceClosurePackageDesignShape {
                closure_package_design_only: true,
                slot_count: evidence_closure_package_entries().len(),
                release_profile_slot_present: true,
                positive_get_order_slot_present: true,
                route_template_recheck_slot_present: true,
                undocumented_2xx_slot_present: true,
                cancel_409_410_slot_present: true,
                all_slots_require_evidence_or_waiver: true,
                all_slots_require_reviewer_acceptance: true,
                source_archive_binding_required: true,
                order_endpoint_calls_allowed_for_closure: false,
                raw_values_exported: false,
            },
        route_template_recheck_plan_design:
            GatewayRealOrderEndpointRouteTemplateRecheckPlanDesignShape {
                route_template_recheck_design_only: true,
                route_count: 2,
                exact_two_route_allowlist_required: true,
                official_docs_or_waiver_required: true,
                reviewer_acceptance_required: true,
                recheck_before_implementation_gate: true,
                route_templates_exported_as_design_data_only: true,
                rendered_routes_exported: false,
                raw_account_or_order_id_exported: false,
                order_endpoint_calls_allowed_for_recheck: false,
            },
        evidence_report_readiness_design:
            GatewayRealOrderEndpointEvidenceReportReadinessDesignShape {
                evidence_report_readiness_design_only: true,
                canonical_replay_golden_vector_sha256: canonical_replay_golden_vectors()
                    .first()
                    .map(|vector| vector.expected_sha256.clone())
                    .unwrap_or_default(),
                canonical_replay_vector_count: canonical_replay_golden_vectors().len(),
                readiness_implemented_tested_count: implementation_gate_readiness_checklist()
                    .iter()
                    .filter(|entry| {
                        entry.status
                            == GatewayRealOrderEndpointImplementationGateReadinessStatus::ImplementedAndTested
                    })
                    .count(),
                readiness_pending_evidence_or_waiver_count: implementation_gate_readiness_checklist()
                    .iter()
                    .filter(|entry| {
                        entry.status
                            == GatewayRealOrderEndpointImplementationGateReadinessStatus::PendingEvidenceOrWaiver
                    })
                    .count(),
                operator_replay_runbook_case_count: operator_replay_runbook_entries().len(),
                raw_values_exported: false,
            },
        durable_checkpoint_capability_design:
            GatewayRealOrderEndpointDurableCheckpointCapabilityDesignShape {
                place_capability_type_internal: true,
                cancel_capability_type_internal: true,
                capability_not_debug_or_serializable: true,
                created_after_sqlite_transition_only: true,
                place_required_event: OrderPathEvent::BeginSubmit,
                place_required_label:
                    GatewayRealOrderEndpointDurableCheckpointLabel::PlaceBeginSubmitPersistedBeforeEndpoint,
                cancel_required_event: OrderPathEvent::RequestCancel,
                cancel_required_label:
                    GatewayRealOrderEndpointDurableCheckpointLabel::CancelRequestCancelPersistedBeforeEndpoint,
            },
        checkpoint_marker_creation_design:
            GatewayRealOrderEndpointCheckpointMarkerCreationDesignShape {
                place_marker_creation_function_private: true,
                cancel_marker_creation_function_private: true,
                creation_requires_endpoint_gate: true,
                creation_requires_sqlite_transition_commit_proof: true,
                proof_bound_to_request_snapshot_fingerprint: true,
                proof_fingerprint_includes_request_client_account_instrument_hashes: true,
                proof_raw_request_values_exported: false,
                marker_single_use_required: true,
                checkpoint_reuse_across_intents_allowed: false,
                creation_rejects_diagnostic_or_report_source: true,
                creation_requires_durable_commit_observed: true,
                creation_requires_transition_event_match: true,
                creation_requires_fingerprint_operation_match: true,
                place_required_event: OrderPathEvent::BeginSubmit,
                cancel_required_event: OrderPathEvent::RequestCancel,
                marker_not_debug_or_serializable: true,
                constructor_count: checkpoint_marker_creation_constructor_count(),
            },
        runtime_ack_id_policy: RuntimeCommandAckIdPolicy::RedactedRuntimeAckOnly,
        scanner_transition_spec: GatewayRealOrderEndpointScannerTransitionSpec {
            current_mode: M3cOrderEndpointScannerTransitionMode::CurrentDenyAllOrderPostDelete,
            future_mode:
                M3cOrderEndpointScannerTransitionMode::FutureExactTwoRouteAllowlistAfterReview,
            exact_place_order_surface_count: 1,
            exact_cancel_order_surface_count: 1,
            approved_module_path,
            allowed_route_template_count: 2,
            negative_tests: crate::m3c_order_endpoint_negative_test_plan(),
            real_post_delete_calls_allowed_now: false,
        },
    }
}

fn place_order_route_shape() -> GatewayRealOrderEndpointInternalRouteShape {
    GatewayRealOrderEndpointInternalRouteShape {
        operation: GatewayRealOrderEndpointOperation::PlaceOrder,
        method_name: "POST",
        route_template: "/v1/accounts/{account_id}/orders",
        gate_marker_required: true,
    }
}

fn cancel_order_route_shape() -> GatewayRealOrderEndpointInternalRouteShape {
    GatewayRealOrderEndpointInternalRouteShape {
        operation: GatewayRealOrderEndpointOperation::CancelOrder,
        method_name: "DELETE",
        route_template: "/v1/accounts/{account_id}/orders/{order_id}",
        gate_marker_required: true,
    }
}

fn redacted_route_diagnostic(
    route: GatewayRealOrderEndpointInternalRouteShape,
) -> GatewayRealOrderEndpointRedactedRouteDiagnostic {
    GatewayRealOrderEndpointRedactedRouteDiagnostic {
        operation: route.operation,
        method_name: route.method_name.to_string(),
        route_template_redacted: true,
        route_template_exported: false,
        gate_marker_required: route.gate_marker_required,
    }
}

fn render_path_from_segments(segments: Vec<String>) -> RenderedOrderEndpointPath {
    RenderedOrderEndpointPath(format!("/{}", segments.join("/")))
}

fn validate_request_part_inputs(
    operation: GatewayRealOrderEndpointOperation,
    allowlist: &OrderEndpointAccountInstrumentAllowlistApproved,
    operator_arm: &OrderEndpointOperatorArmApproved,
    checkpoint: &OrderEndpointDurableStateCheckpoint,
) -> Result<(), GatewayRealOrderEndpointApprovedPartsError> {
    if !(allowlist.account_allowlisted && allowlist.instrument_allowlisted) {
        return Err(GatewayRealOrderEndpointApprovedPartsError::AccountInstrumentAllowlist);
    }
    if !(operator_arm.operator_arm_validated && operator_arm.one_shot_arm) {
        return Err(GatewayRealOrderEndpointApprovedPartsError::OperatorArm);
    }
    if !checkpoint.intent_recorded_before_endpoint {
        return Err(GatewayRealOrderEndpointApprovedPartsError::DurableStateCheckpoint);
    }
    if checkpoint.label != expected_checkpoint_label(operation) {
        return Err(GatewayRealOrderEndpointApprovedPartsError::DurableStateCheckpoint);
    }
    Ok(())
}

fn expected_checkpoint_label(
    operation: GatewayRealOrderEndpointOperation,
) -> GatewayRealOrderEndpointDurableCheckpointLabel {
    match operation {
        GatewayRealOrderEndpointOperation::PlaceOrder => {
            GatewayRealOrderEndpointDurableCheckpointLabel::PlaceBeginSubmitPersistedBeforeEndpoint
        }
        GatewayRealOrderEndpointOperation::CancelOrder => {
            GatewayRealOrderEndpointDurableCheckpointLabel::CancelRequestCancelPersistedBeforeEndpoint
        }
    }
}

fn build_place_approved_request_parts(
    _gate: &EndpointGateApproved,
    approved_spec: &broker_finam::FinamPlaceOrderRequestSpec,
    allowlist: &OrderEndpointAccountInstrumentAllowlistApproved,
    operator_arm: &OrderEndpointOperatorArmApproved,
    checkpoint: &OrderEndpointDurableStateCheckpoint,
) -> Result<ApprovedOrderEndpointRequestParts, GatewayRealOrderEndpointApprovedPartsError> {
    let route = place_order_route_shape();
    validate_request_part_inputs(route.operation, allowlist, operator_arm, checkpoint)?;
    Ok(ApprovedOrderEndpointRequestParts {
        operation: route.operation,
        method_name: route.method_name,
        rendered_path: render_path_from_segments(approved_spec.rest_path_segments()),
        approved_request_spec: ApprovedOrderEndpointRequestSpec::Place(approved_spec.clone()),
        account_instrument_allowlist_approved: true,
        operator_arm_approved: true,
        durable_state_checkpoint_present: true,
        durable_checkpoint_label: checkpoint.label,
    })
}

fn build_cancel_approved_request_parts(
    _gate: &EndpointGateApproved,
    approved_spec: &broker_finam::FinamCancelOrderRequestSpec,
    allowlist: &OrderEndpointAccountInstrumentAllowlistApproved,
    operator_arm: &OrderEndpointOperatorArmApproved,
    checkpoint: &OrderEndpointDurableStateCheckpoint,
) -> Result<ApprovedOrderEndpointRequestParts, GatewayRealOrderEndpointApprovedPartsError> {
    let route = cancel_order_route_shape();
    validate_request_part_inputs(route.operation, allowlist, operator_arm, checkpoint)?;
    Ok(ApprovedOrderEndpointRequestParts {
        operation: route.operation,
        method_name: route.method_name,
        rendered_path: render_path_from_segments(approved_spec.rest_path_segments()),
        approved_request_spec: ApprovedOrderEndpointRequestSpec::Cancel(approved_spec.clone()),
        account_instrument_allowlist_approved: true,
        operator_arm_approved: true,
        durable_state_checkpoint_present: true,
        durable_checkpoint_label: checkpoint.label,
    })
}

fn approved_request_parts_redacted_diagnostic(
    parts: &ApprovedOrderEndpointRequestParts,
) -> GatewayRealOrderEndpointApprovedPartsDiagnostic {
    let (account_id_present, account_id_len, order_id_present, order_id_len, symbol_len) =
        match &parts.approved_request_spec {
            ApprovedOrderEndpointRequestSpec::Place(spec) => (
                !spec.account_id.is_empty(),
                spec.account_id.len(),
                false,
                None,
                Some(spec.body.symbol.len()),
            ),
            ApprovedOrderEndpointRequestSpec::Cancel(spec) => (
                !spec.account_id.is_empty(),
                spec.account_id.len(),
                !spec.order_id.is_empty(),
                Some(spec.order_id.len()),
                None,
            ),
        };

    GatewayRealOrderEndpointApprovedPartsDiagnostic {
        operation: parts.operation,
        method_name: parts.method_name.to_string(),
        rendered_path_present: !parts.rendered_path.0.is_empty(),
        rendered_path_redacted: true,
        rendered_path_exported: false,
        raw_body_exported: false,
        account_id_present,
        account_id_len,
        order_id_present,
        order_id_len,
        symbol_present: symbol_len.is_some_and(|len| len > 0),
        symbol_len,
        account_instrument_allowlist_approved: parts.account_instrument_allowlist_approved,
        operator_arm_approved: parts.operator_arm_approved,
        durable_state_checkpoint_present: parts.durable_state_checkpoint_present,
        durable_checkpoint_label: parts.durable_checkpoint_label,
    }
}

fn approved_request_parts_consumer_redacted_diagnostic(
    parts: ApprovedOrderEndpointRequestParts,
) -> GatewayRealOrderEndpointConsumerDiagnostic {
    let parts_diagnostic = approved_request_parts_redacted_diagnostic(&parts);
    GatewayRealOrderEndpointConsumerDiagnostic {
        operation: parts_diagnostic.operation,
        method_name: parts_diagnostic.method_name,
        accepted_approved_request_parts: true,
        endpoint_gate_required: true,
        network_enabled: false,
        rendered_path_present: parts_diagnostic.rendered_path_present,
        rendered_path_redacted: true,
        rendered_path_exported: false,
        raw_body_exported: false,
        runtime_ack_redacted_only: true,
        account_id_present: parts_diagnostic.account_id_present,
        account_id_len: parts_diagnostic.account_id_len,
        order_id_present: parts_diagnostic.order_id_present,
        order_id_len: parts_diagnostic.order_id_len,
        symbol_present: parts_diagnostic.symbol_present,
        symbol_len: parts_diagnostic.symbol_len,
    }
}

fn consume_approved_request_parts_for_future_endpoint(
    _gate: &EndpointGateApproved,
    parts: ApprovedOrderEndpointRequestParts,
) -> GatewayRealOrderEndpointConsumerDiagnostic {
    approved_request_parts_consumer_redacted_diagnostic(parts)
}

fn future_send_result_redacted_diagnostic(
    parts: ApprovedOrderEndpointRequestParts,
    outcome: GatewayRealOrderEndpointFutureSendOutcome,
) -> GatewayRealOrderEndpointFutureSendDiagnostic {
    let parts_diagnostic = approved_request_parts_redacted_diagnostic(&parts);
    GatewayRealOrderEndpointFutureSendDiagnostic {
        outcome,
        operation: parts_diagnostic.operation,
        method_name: parts_diagnostic.method_name,
        endpoint_gate_required: true,
        request_parts_consumed: true,
        request_parts_reuse_after_outcome_allowed: false,
        network_enabled: false,
        rendered_path_present: parts_diagnostic.rendered_path_present,
        rendered_path_redacted: true,
        rendered_path_exported: false,
        raw_body_exported: false,
        account_id_present: parts_diagnostic.account_id_present,
        account_id_len: parts_diagnostic.account_id_len,
        order_id_present: parts_diagnostic.order_id_present,
        order_id_len: parts_diagnostic.order_id_len,
        symbol_present: parts_diagnostic.symbol_present,
        symbol_len: parts_diagnostic.symbol_len,
        durable_checkpoint_label: parts_diagnostic.durable_checkpoint_label,
        retry_after_timeout_unknown_allowed: false,
        state_machine_transition_required: true,
        state_machine_bypass_allowed: false,
        runtime_ack_redacted_only: true,
    }
}

fn classify_future_send_attempt_result(
    _gate: &EndpointGateApproved,
    parts: ApprovedOrderEndpointRequestParts,
    outcome: GatewayRealOrderEndpointFutureSendOutcome,
) -> GatewayRealOrderEndpointFutureSendDiagnostic {
    future_send_result_redacted_diagnostic(parts, outcome)
}

fn classify_accepted_result_after_future_send(
    _gate: &EndpointGateApproved,
    parts: ApprovedOrderEndpointRequestParts,
    accepted_response: GatewayRealOrderEndpointAcceptedResponseShape,
) -> GatewayRealOrderEndpointAcceptedResultDiagnostic {
    GatewayRealOrderEndpointAcceptedResultDiagnostic {
        kind: accepted_response.kind,
        operation: parts.operation,
        method_name: parts.method_name.to_string(),
        endpoint_gate_required: true,
        request_parts_consumed: true,
        accepted_response_shape_consumed: true,
        uses_accepted_broker_id_policy_matrix: true,
        raw_broker_order_id_exported: false,
        broker_order_id_redacted: true,
        state_machine_transition_required: true,
        state_machine_bypass_allowed: false,
        runtime_ack_redacted_only: true,
    }
}

fn captured_response_error_envelope_diagnostic(
    _gate: &EndpointGateApproved,
    parts: ApprovedOrderEndpointRequestParts,
    kind: GatewayRealOrderEndpointCapturedEnvelopeKind,
    transport_category: Option<GatewayRealOrderEndpointTransportCategory>,
) -> GatewayRealOrderEndpointCapturedResponseEnvelopeDiagnostic {
    GatewayRealOrderEndpointCapturedResponseEnvelopeDiagnostic {
        kind,
        operation: parts.operation,
        method_name: parts.method_name.to_string(),
        endpoint_gate_required: true,
        request_parts_consumed: true,
        request_snapshot_fingerprint_present: true,
        status_code_present: kind != GatewayRealOrderEndpointCapturedEnvelopeKind::TransportError,
        body_present: kind != GatewayRealOrderEndpointCapturedEnvelopeKind::TransportError,
        body_len_recorded: kind != GatewayRealOrderEndpointCapturedEnvelopeKind::TransportError,
        body_sha256_recorded: kind != GatewayRealOrderEndpointCapturedEnvelopeKind::TransportError,
        transport_category,
        error_len_recorded: transport_category.is_some(),
        error_sha256_recorded: transport_category.is_some(),
        raw_path_exported: false,
        raw_body_exported: false,
        raw_error_exported: false,
        runtime_ack_redacted_only: true,
    }
}

fn bind_place_endpoint_attempt_journal(
    _gate: &EndpointGateApproved,
    parts: ApprovedOrderEndpointRequestParts,
    _checkpoint_marker: PlaceEndpointDurableCheckpointApproved,
    _captured_envelope: GatewayRealOrderEndpointCapturedEnvelopeRecord,
    _outcome: GatewayRealOrderEndpointFutureSendOutcome,
) -> GatewayRealOrderEndpointAttemptDiagnostic {
    endpoint_attempt_redacted_diagnostic(parts.operation)
}

fn bind_cancel_endpoint_attempt_journal(
    _gate: &EndpointGateApproved,
    parts: ApprovedOrderEndpointRequestParts,
    _checkpoint_marker: CancelEndpointDurableCheckpointApproved,
    _captured_envelope: GatewayRealOrderEndpointCapturedEnvelopeRecord,
    _outcome: GatewayRealOrderEndpointFutureSendOutcome,
) -> GatewayRealOrderEndpointAttemptDiagnostic {
    endpoint_attempt_redacted_diagnostic(parts.operation)
}

fn endpoint_attempt_redacted_diagnostic(
    operation: GatewayRealOrderEndpointOperation,
) -> GatewayRealOrderEndpointAttemptDiagnostic {
    GatewayRealOrderEndpointAttemptDiagnostic {
        operation,
        endpoint_attempt_id_hash_present: true,
        endpoint_attempt_id_sha256_len: 64,
        request_snapshot_fingerprint_present: true,
        request_parts_bound: true,
        checkpoint_marker_bound: true,
        captured_envelope_bound: true,
        outcome_classifier_bound: true,
        state_machine_transition_required: true,
        state_machine_bypass_allowed: false,
        raw_path_exported: false,
        raw_body_exported: false,
        raw_error_exported: false,
        runtime_ack_redacted_only: true,
    }
}

fn append_place_durable_endpoint_attempt_journal(
    _gate: &EndpointGateApproved,
    input: GatewayRealOrderEndpointDurableAttemptJournalAppendInput,
    _checkpoint_marker: PlaceEndpointDurableCheckpointApproved,
) -> GatewayRealOrderEndpointDurableAttemptJournalDiagnostic {
    durable_endpoint_attempt_journal_redacted_diagnostic(input.parts.operation)
}

fn append_cancel_durable_endpoint_attempt_journal(
    _gate: &EndpointGateApproved,
    input: GatewayRealOrderEndpointDurableAttemptJournalAppendInput,
    _checkpoint_marker: CancelEndpointDurableCheckpointApproved,
) -> GatewayRealOrderEndpointDurableAttemptJournalDiagnostic {
    durable_endpoint_attempt_journal_redacted_diagnostic(input.parts.operation)
}

fn durable_endpoint_attempt_journal_redacted_diagnostic(
    operation: GatewayRealOrderEndpointOperation,
) -> GatewayRealOrderEndpointDurableAttemptJournalDiagnostic {
    GatewayRealOrderEndpointDurableAttemptJournalDiagnostic {
        operation,
        endpoint_attempt_id_hash_present: true,
        endpoint_attempt_id_sha256_len: 64,
        request_fingerprint_bound: true,
        checkpoint_proof_fingerprint_bound: true,
        captured_envelope_fingerprint_bound: true,
        outcome_fingerprint_bound: true,
        state_transition_result_fingerprint_bound: true,
        ack_diagnostic_fingerprint_bound: true,
        append_committed_after_state_transition: true,
        state_machine_transition_required: true,
        state_machine_bypass_allowed: false,
        raw_endpoint_attempt_id_exported: false,
        raw_request_values_exported: false,
        raw_broker_order_id_exported: false,
        raw_path_exported: false,
        raw_body_exported: false,
        raw_error_exported: false,
        runtime_ack_redacted_only: true,
    }
}

fn approved_request_parts_constructor_count() -> usize {
    let _place: fn(
        &EndpointGateApproved,
        &broker_finam::FinamPlaceOrderRequestSpec,
        &OrderEndpointAccountInstrumentAllowlistApproved,
        &OrderEndpointOperatorArmApproved,
        &OrderEndpointDurableStateCheckpoint,
    ) -> Result<
        ApprovedOrderEndpointRequestParts,
        GatewayRealOrderEndpointApprovedPartsError,
    > = build_place_approved_request_parts;
    let _cancel: fn(
        &EndpointGateApproved,
        &broker_finam::FinamCancelOrderRequestSpec,
        &OrderEndpointAccountInstrumentAllowlistApproved,
        &OrderEndpointOperatorArmApproved,
        &OrderEndpointDurableStateCheckpoint,
    ) -> Result<
        ApprovedOrderEndpointRequestParts,
        GatewayRealOrderEndpointApprovedPartsError,
    > = build_cancel_approved_request_parts;
    let _diagnostic: fn(
        &ApprovedOrderEndpointRequestParts,
    ) -> GatewayRealOrderEndpointApprovedPartsDiagnostic =
        approved_request_parts_redacted_diagnostic;
    2
}

fn approved_request_parts_consumer_count() -> usize {
    let _consumer: fn(
        &EndpointGateApproved,
        ApprovedOrderEndpointRequestParts,
    ) -> GatewayRealOrderEndpointConsumerDiagnostic =
        consume_approved_request_parts_for_future_endpoint;
    1
}

fn future_send_outcomes() -> [GatewayRealOrderEndpointFutureSendOutcome; 8] {
    [
        GatewayRealOrderEndpointFutureSendOutcome::Accepted,
        GatewayRealOrderEndpointFutureSendOutcome::Rejected,
        GatewayRealOrderEndpointFutureSendOutcome::TimeoutUnknownPending,
        GatewayRealOrderEndpointFutureSendOutcome::RateLimited,
        GatewayRealOrderEndpointFutureSendOutcome::Maintenance,
        GatewayRealOrderEndpointFutureSendOutcome::Unauthorized,
        GatewayRealOrderEndpointFutureSendOutcome::DecodeError,
        GatewayRealOrderEndpointFutureSendOutcome::TransportError,
    ]
}

fn future_send_outcome_count() -> usize {
    future_send_outcomes().len()
}

fn future_send_result_classifier_count() -> usize {
    let _classifier: fn(
        &EndpointGateApproved,
        ApprovedOrderEndpointRequestParts,
        GatewayRealOrderEndpointFutureSendOutcome,
    ) -> GatewayRealOrderEndpointFutureSendDiagnostic = classify_future_send_attempt_result;
    1
}

fn accepted_result_kinds() -> [GatewayRealOrderEndpointAcceptedResultKind; 4] {
    [
        GatewayRealOrderEndpointAcceptedResultKind::WithBrokerOrderId,
        GatewayRealOrderEndpointAcceptedResultKind::WithoutBrokerOrderId,
        GatewayRealOrderEndpointAcceptedResultKind::EmptyBrokerOrderId,
        GatewayRealOrderEndpointAcceptedResultKind::BrokerOrderIdMismatch,
    ]
}

fn accepted_result_kind_count() -> usize {
    accepted_result_kinds().len()
}

fn accepted_result_classifier_count() -> usize {
    let _classifier: fn(
        &EndpointGateApproved,
        ApprovedOrderEndpointRequestParts,
        GatewayRealOrderEndpointAcceptedResponseShape,
    ) -> GatewayRealOrderEndpointAcceptedResultDiagnostic =
        classify_accepted_result_after_future_send;
    1
}

fn captured_envelope_diagnostic_count() -> usize {
    let _diagnostic: fn(
        &EndpointGateApproved,
        ApprovedOrderEndpointRequestParts,
        GatewayRealOrderEndpointCapturedEnvelopeKind,
        Option<GatewayRealOrderEndpointTransportCategory>,
    ) -> GatewayRealOrderEndpointCapturedResponseEnvelopeDiagnostic =
        captured_response_error_envelope_diagnostic;
    1
}

fn endpoint_attempt_journal_binding_constructor_count() -> usize {
    let _place: fn(
        &EndpointGateApproved,
        ApprovedOrderEndpointRequestParts,
        PlaceEndpointDurableCheckpointApproved,
        GatewayRealOrderEndpointCapturedEnvelopeRecord,
        GatewayRealOrderEndpointFutureSendOutcome,
    ) -> GatewayRealOrderEndpointAttemptDiagnostic = bind_place_endpoint_attempt_journal;
    let _cancel: fn(
        &EndpointGateApproved,
        ApprovedOrderEndpointRequestParts,
        CancelEndpointDurableCheckpointApproved,
        GatewayRealOrderEndpointCapturedEnvelopeRecord,
        GatewayRealOrderEndpointFutureSendOutcome,
    ) -> GatewayRealOrderEndpointAttemptDiagnostic = bind_cancel_endpoint_attempt_journal;
    2
}

fn endpoint_attempt_diagnostic_count() -> usize {
    let _diagnostic: fn(
        GatewayRealOrderEndpointOperation,
    ) -> GatewayRealOrderEndpointAttemptDiagnostic = endpoint_attempt_redacted_diagnostic;
    1
}

fn durable_attempt_journal_append_constructor_count() -> usize {
    let _place: fn(
        &EndpointGateApproved,
        GatewayRealOrderEndpointDurableAttemptJournalAppendInput,
        PlaceEndpointDurableCheckpointApproved,
    ) -> GatewayRealOrderEndpointDurableAttemptJournalDiagnostic =
        append_place_durable_endpoint_attempt_journal;
    let _cancel: fn(
        &EndpointGateApproved,
        GatewayRealOrderEndpointDurableAttemptJournalAppendInput,
        CancelEndpointDurableCheckpointApproved,
    ) -> GatewayRealOrderEndpointDurableAttemptJournalDiagnostic =
        append_cancel_durable_endpoint_attempt_journal;
    2
}

fn durable_attempt_journal_diagnostic_count() -> usize {
    let _diagnostic: fn(
        GatewayRealOrderEndpointOperation,
    ) -> GatewayRealOrderEndpointDurableAttemptJournalDiagnostic =
        durable_endpoint_attempt_journal_redacted_diagnostic;
    1
}

fn transport_categories() -> [GatewayRealOrderEndpointTransportCategory; 5] {
    [
        GatewayRealOrderEndpointTransportCategory::DnsOrConnectError,
        GatewayRealOrderEndpointTransportCategory::TlsError,
        GatewayRealOrderEndpointTransportCategory::HttpSendError,
        GatewayRealOrderEndpointTransportCategory::BodyReadError,
        GatewayRealOrderEndpointTransportCategory::Timeout,
    ]
}

fn transport_category_count() -> usize {
    transport_categories().len()
}

pub fn transport_category_policy_matrix(
) -> Vec<GatewayRealOrderEndpointTransportCategoryPolicyEntry> {
    use GatewayRealOrderEndpointFutureSendOutcome as Outcome;
    use GatewayRealOrderEndpointOperatorPolicy as OperatorPolicy;
    use GatewayRealOrderEndpointTransportCategory as Category;
    use GatewayRealOrderEndpointTransportStateSemantics as Semantics;

    vec![
        GatewayRealOrderEndpointTransportCategoryPolicyEntry {
            category: Category::DnsOrConnectError,
            send_outcome: Outcome::TransportError,
            state_semantics: Semantics::NonTimeoutTransportFailure,
            place_event: OrderPathEvent::RequireManualIntervention,
            place_state: OrderPathState::ManualInterventionRequired,
            cancel_event: OrderPathEvent::RequireManualIntervention,
            cancel_state: OrderPathState::ManualInterventionRequired,
            error_kind: Some(OrderPathErrorKind::Unknown),
            ack_status: CommandAckStatus::Error,
            place_ack_reason_code: Some(CommandAckReasonCode::ManualInterventionRequired),
            cancel_ack_reason_code: Some(CommandAckReasonCode::ManualInterventionRequired),
            operator_policy: OperatorPolicy::DegradeAndManualIntervention,
            operator_disarm_signal: Some(OperatorDisarmSignal::GatewayDegraded),
            reconciliation_required: false,
            backoff_required: true,
            manual_intervention_required: true,
            no_blind_retry: true,
            timeout_unknown_pending_semantics: false,
            non_timeout_transport_semantics: true,
            state_machine_transition_required: true,
            result_diagnostic_can_bypass_state_machine: false,
            runtime_ack_redacted_only: true,
        },
        GatewayRealOrderEndpointTransportCategoryPolicyEntry {
            category: Category::TlsError,
            send_outcome: Outcome::TransportError,
            state_semantics: Semantics::NonTimeoutTransportFailure,
            place_event: OrderPathEvent::RequireManualIntervention,
            place_state: OrderPathState::ManualInterventionRequired,
            cancel_event: OrderPathEvent::RequireManualIntervention,
            cancel_state: OrderPathState::ManualInterventionRequired,
            error_kind: Some(OrderPathErrorKind::Unknown),
            ack_status: CommandAckStatus::Error,
            place_ack_reason_code: Some(CommandAckReasonCode::ManualInterventionRequired),
            cancel_ack_reason_code: Some(CommandAckReasonCode::ManualInterventionRequired),
            operator_policy: OperatorPolicy::DegradeAndManualIntervention,
            operator_disarm_signal: Some(OperatorDisarmSignal::GatewayDegraded),
            reconciliation_required: false,
            backoff_required: true,
            manual_intervention_required: true,
            no_blind_retry: true,
            timeout_unknown_pending_semantics: false,
            non_timeout_transport_semantics: true,
            state_machine_transition_required: true,
            result_diagnostic_can_bypass_state_machine: false,
            runtime_ack_redacted_only: true,
        },
        GatewayRealOrderEndpointTransportCategoryPolicyEntry {
            category: Category::HttpSendError,
            send_outcome: Outcome::TransportError,
            state_semantics: Semantics::NonTimeoutTransportFailure,
            place_event: OrderPathEvent::RequireManualIntervention,
            place_state: OrderPathState::ManualInterventionRequired,
            cancel_event: OrderPathEvent::RequireManualIntervention,
            cancel_state: OrderPathState::ManualInterventionRequired,
            error_kind: Some(OrderPathErrorKind::Unknown),
            ack_status: CommandAckStatus::Error,
            place_ack_reason_code: Some(CommandAckReasonCode::ManualInterventionRequired),
            cancel_ack_reason_code: Some(CommandAckReasonCode::ManualInterventionRequired),
            operator_policy: OperatorPolicy::TransportCategoryManualIntervention,
            operator_disarm_signal: Some(OperatorDisarmSignal::GatewayDegraded),
            reconciliation_required: true,
            backoff_required: true,
            manual_intervention_required: true,
            no_blind_retry: true,
            timeout_unknown_pending_semantics: false,
            non_timeout_transport_semantics: true,
            state_machine_transition_required: true,
            result_diagnostic_can_bypass_state_machine: false,
            runtime_ack_redacted_only: true,
        },
        GatewayRealOrderEndpointTransportCategoryPolicyEntry {
            category: Category::BodyReadError,
            send_outcome: Outcome::TransportError,
            state_semantics: Semantics::NonTimeoutTransportFailure,
            place_event: OrderPathEvent::RequireManualIntervention,
            place_state: OrderPathState::ManualInterventionRequired,
            cancel_event: OrderPathEvent::RequireManualIntervention,
            cancel_state: OrderPathState::ManualInterventionRequired,
            error_kind: Some(OrderPathErrorKind::Unknown),
            ack_status: CommandAckStatus::Error,
            place_ack_reason_code: Some(CommandAckReasonCode::ManualInterventionRequired),
            cancel_ack_reason_code: Some(CommandAckReasonCode::ManualInterventionRequired),
            operator_policy: OperatorPolicy::TransportCategoryManualIntervention,
            operator_disarm_signal: Some(OperatorDisarmSignal::GatewayDegraded),
            reconciliation_required: true,
            backoff_required: false,
            manual_intervention_required: true,
            no_blind_retry: true,
            timeout_unknown_pending_semantics: false,
            non_timeout_transport_semantics: true,
            state_machine_transition_required: true,
            result_diagnostic_can_bypass_state_machine: false,
            runtime_ack_redacted_only: true,
        },
        GatewayRealOrderEndpointTransportCategoryPolicyEntry {
            category: Category::Timeout,
            send_outcome: Outcome::TimeoutUnknownPending,
            state_semantics: Semantics::TimeoutUnknownPending,
            place_event: OrderPathEvent::SubmitTimedOut,
            place_state: OrderPathState::TimeoutUnknownPending,
            cancel_event: OrderPathEvent::CancelTimedOut,
            cancel_state: OrderPathState::CancelTimeoutUnknownPending,
            error_kind: Some(OrderPathErrorKind::TransportTimeout),
            ack_status: CommandAckStatus::Timeout,
            place_ack_reason_code: Some(CommandAckReasonCode::TimeoutUnknownPending),
            cancel_ack_reason_code: Some(CommandAckReasonCode::CancelTimeoutUnknownPending),
            operator_policy: OperatorPolicy::DisarmAndOperatorIntervention,
            operator_disarm_signal: Some(OperatorDisarmSignal::UnknownPendingOrder),
            reconciliation_required: true,
            backoff_required: false,
            manual_intervention_required: true,
            no_blind_retry: true,
            timeout_unknown_pending_semantics: true,
            non_timeout_transport_semantics: false,
            state_machine_transition_required: true,
            result_diagnostic_can_bypass_state_machine: false,
            runtime_ack_redacted_only: true,
        },
    ]
}

// A matrix row is intentionally explicit: each call site lists the broker
// status/body shape, future outcome, order-path transition, ACK shape, and
// operator policy in one place for reviewability.
#[allow(clippy::too_many_arguments)]
fn http_status_outcome_entry(
    operation: GatewayRealOrderEndpointOperation,
    status_code: u16,
    body_shape: GatewayRealOrderEndpointHttpBodyShape,
    outcome: GatewayRealOrderEndpointFutureSendOutcome,
    accepted_result_kind: Option<GatewayRealOrderEndpointAcceptedResultKind>,
    cancel_accepted_id_policy: Option<GatewayRealOrderEndpointCancelAcceptedIdPolicy>,
    order_path_event: OrderPathEvent,
    order_path_state: OrderPathState,
    ack_status: CommandAckStatus,
    ack_reason_code: Option<CommandAckReasonCode>,
    operator_disarm_signal: Option<OperatorDisarmSignal>,
) -> GatewayRealOrderEndpointHttpStatusOutcomeEntry {
    GatewayRealOrderEndpointHttpStatusOutcomeEntry {
        operation,
        status_code,
        body_shape,
        envelope_kind: if matches!(
            body_shape,
            GatewayRealOrderEndpointHttpBodyShape::BrokerReject
                | GatewayRealOrderEndpointHttpBodyShape::Unauthorized
                | GatewayRealOrderEndpointHttpBodyShape::Timeout
                | GatewayRealOrderEndpointHttpBodyShape::RateLimit
                | GatewayRealOrderEndpointHttpBodyShape::Maintenance
        ) {
            GatewayRealOrderEndpointCapturedEnvelopeKind::BrokerErrorResponse
        } else {
            GatewayRealOrderEndpointCapturedEnvelopeKind::AcceptedResponse
        },
        outcome,
        accepted_result_kind,
        cancel_accepted_id_policy,
        order_path_event,
        order_path_state,
        ack_status,
        ack_reason_code,
        operator_disarm_signal,
        state_machine_transition_required: true,
        captured_envelope_required: true,
        endpoint_attempt_journal_required: true,
        no_blind_retry: true,
        raw_path_exported: false,
        raw_body_exported: false,
        raw_error_exported: false,
    }
}

pub fn place_http_status_outcome_matrix() -> Vec<GatewayRealOrderEndpointHttpStatusOutcomeEntry> {
    use GatewayRealOrderEndpointAcceptedResultKind as AcceptedKind;
    use GatewayRealOrderEndpointFutureSendOutcome as Outcome;
    use GatewayRealOrderEndpointHttpBodyShape as Body;

    vec![
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            200,
            Body::AcceptedWithBrokerOrderId,
            Outcome::Accepted,
            Some(AcceptedKind::WithBrokerOrderId),
            None,
            OrderPathEvent::SubmitAccepted,
            OrderPathState::Submitted,
            CommandAckStatus::Submitted,
            None,
            None,
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            200,
            Body::AcceptedWithoutBrokerOrderId,
            Outcome::Accepted,
            Some(AcceptedKind::WithoutBrokerOrderId),
            None,
            OrderPathEvent::SubmitAcceptedWithoutBrokerOrderId,
            OrderPathState::SubmittedPendingBrokerOrderId,
            CommandAckStatus::UnknownPending,
            Some(CommandAckReasonCode::ReconciliationRequired),
            Some(OperatorDisarmSignal::AcceptedWithoutBrokerOrderId),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            200,
            Body::AcceptedEmptyBrokerOrderId,
            Outcome::Accepted,
            Some(AcceptedKind::EmptyBrokerOrderId),
            None,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::ResponseDecodeError),
            Some(OperatorDisarmSignal::OrderEndpointDecodeError),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            200,
            Body::AcceptedBrokerOrderIdMismatch,
            Outcome::Accepted,
            Some(AcceptedKind::BrokerOrderIdMismatch),
            None,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::UnknownPending,
            Some(CommandAckReasonCode::ManualInterventionRequired),
            Some(OperatorDisarmSignal::ReconciliationConflict),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            400,
            Body::BrokerReject,
            Outcome::Rejected,
            None,
            None,
            OrderPathEvent::BrokerReject,
            OrderPathState::BrokerRejected,
            CommandAckStatus::Rejected,
            Some(CommandAckReasonCode::BrokerRejected),
            None,
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            422,
            Body::BrokerReject,
            Outcome::Rejected,
            None,
            None,
            OrderPathEvent::BrokerReject,
            OrderPathState::BrokerRejected,
            CommandAckStatus::Rejected,
            Some(CommandAckReasonCode::BrokerRejected),
            None,
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            401,
            Body::Unauthorized,
            Outcome::Unauthorized,
            None,
            None,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::Unauthorized),
            Some(OperatorDisarmSignal::OrderEndpointUnauthorized),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            403,
            Body::Unauthorized,
            Outcome::Unauthorized,
            None,
            None,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::Unauthorized),
            Some(OperatorDisarmSignal::OrderEndpointUnauthorized),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            408,
            Body::Timeout,
            Outcome::TimeoutUnknownPending,
            None,
            None,
            OrderPathEvent::SubmitTimedOut,
            OrderPathState::TimeoutUnknownPending,
            CommandAckStatus::Timeout,
            Some(CommandAckReasonCode::TimeoutUnknownPending),
            Some(OperatorDisarmSignal::UnknownPendingOrder),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            504,
            Body::Timeout,
            Outcome::TimeoutUnknownPending,
            None,
            None,
            OrderPathEvent::SubmitTimedOut,
            OrderPathState::TimeoutUnknownPending,
            CommandAckStatus::Timeout,
            Some(CommandAckReasonCode::TimeoutUnknownPending),
            Some(OperatorDisarmSignal::UnknownPendingOrder),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            429,
            Body::RateLimit,
            Outcome::RateLimited,
            None,
            None,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::RateLimited),
            Some(OperatorDisarmSignal::OrderEndpointRateLimited),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            500,
            Body::Maintenance,
            Outcome::Maintenance,
            None,
            None,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::BrokerMaintenance),
            Some(OperatorDisarmSignal::OrderEndpointMaintenance),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            502,
            Body::Maintenance,
            Outcome::Maintenance,
            None,
            None,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::BrokerMaintenance),
            Some(OperatorDisarmSignal::OrderEndpointMaintenance),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            503,
            Body::Maintenance,
            Outcome::Maintenance,
            None,
            None,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::BrokerMaintenance),
            Some(OperatorDisarmSignal::OrderEndpointMaintenance),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            200,
            Body::MalformedBody,
            Outcome::DecodeError,
            None,
            None,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::ResponseDecodeError),
            Some(OperatorDisarmSignal::OrderEndpointDecodeError),
        ),
    ]
}

pub fn cancel_http_status_outcome_matrix() -> Vec<GatewayRealOrderEndpointHttpStatusOutcomeEntry> {
    use GatewayRealOrderEndpointCancelAcceptedIdPolicy as CancelPolicy;
    use GatewayRealOrderEndpointFutureSendOutcome as Outcome;
    use GatewayRealOrderEndpointHttpBodyShape as Body;

    vec![
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            200,
            Body::AcceptedWithBrokerOrderId,
            Outcome::Accepted,
            None,
            Some(CancelPolicy::MatchingBrokerOrderId),
            OrderPathEvent::CancelAccepted,
            OrderPathState::CancelSubmitted,
            CommandAckStatus::Submitted,
            None,
            None,
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            200,
            Body::AcceptedWithoutBrokerOrderId,
            Outcome::Accepted,
            None,
            Some(CancelPolicy::MissingBrokerOrderIdAcceptedPendingReconciliation),
            OrderPathEvent::CancelAccepted,
            OrderPathState::CancelSubmitted,
            CommandAckStatus::UnknownPending,
            Some(CommandAckReasonCode::ReconciliationRequired),
            Some(OperatorDisarmSignal::AcceptedWithoutBrokerOrderId),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            200,
            Body::AcceptedEmptyBrokerOrderId,
            Outcome::Accepted,
            None,
            Some(CancelPolicy::MissingBrokerOrderIdAcceptedPendingReconciliation),
            OrderPathEvent::CancelAccepted,
            OrderPathState::CancelSubmitted,
            CommandAckStatus::UnknownPending,
            Some(CommandAckReasonCode::ReconciliationRequired),
            Some(OperatorDisarmSignal::AcceptedWithoutBrokerOrderId),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            200,
            Body::AcceptedBrokerOrderIdMismatch,
            Outcome::Accepted,
            None,
            Some(CancelPolicy::BrokerOrderIdMismatchManualIntervention),
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::UnknownPending,
            Some(CommandAckReasonCode::ManualInterventionRequired),
            Some(OperatorDisarmSignal::CancelBrokerOrderIdMismatch),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            400,
            Body::BrokerReject,
            Outcome::Rejected,
            None,
            None,
            OrderPathEvent::CancelRejected,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Rejected,
            Some(CommandAckReasonCode::BrokerRejected),
            None,
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            422,
            Body::BrokerReject,
            Outcome::Rejected,
            None,
            None,
            OrderPathEvent::CancelRejected,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Rejected,
            Some(CommandAckReasonCode::BrokerRejected),
            None,
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            401,
            Body::Unauthorized,
            Outcome::Unauthorized,
            None,
            None,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::Unauthorized),
            Some(OperatorDisarmSignal::OrderEndpointUnauthorized),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            403,
            Body::Unauthorized,
            Outcome::Unauthorized,
            None,
            None,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::Unauthorized),
            Some(OperatorDisarmSignal::OrderEndpointUnauthorized),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            408,
            Body::Timeout,
            Outcome::TimeoutUnknownPending,
            None,
            None,
            OrderPathEvent::CancelTimedOut,
            OrderPathState::CancelTimeoutUnknownPending,
            CommandAckStatus::Timeout,
            Some(CommandAckReasonCode::CancelTimeoutUnknownPending),
            Some(OperatorDisarmSignal::CancelTimeoutUnknownPending),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            504,
            Body::Timeout,
            Outcome::TimeoutUnknownPending,
            None,
            None,
            OrderPathEvent::CancelTimedOut,
            OrderPathState::CancelTimeoutUnknownPending,
            CommandAckStatus::Timeout,
            Some(CommandAckReasonCode::CancelTimeoutUnknownPending),
            Some(OperatorDisarmSignal::CancelTimeoutUnknownPending),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            429,
            Body::RateLimit,
            Outcome::RateLimited,
            None,
            None,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::RateLimited),
            Some(OperatorDisarmSignal::OrderEndpointRateLimited),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            500,
            Body::Maintenance,
            Outcome::Maintenance,
            None,
            None,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::BrokerMaintenance),
            Some(OperatorDisarmSignal::OrderEndpointMaintenance),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            502,
            Body::Maintenance,
            Outcome::Maintenance,
            None,
            None,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::BrokerMaintenance),
            Some(OperatorDisarmSignal::OrderEndpointMaintenance),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            503,
            Body::Maintenance,
            Outcome::Maintenance,
            None,
            None,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::BrokerMaintenance),
            Some(OperatorDisarmSignal::OrderEndpointMaintenance),
        ),
        http_status_outcome_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            200,
            Body::MalformedBody,
            Outcome::DecodeError,
            None,
            None,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::ResponseDecodeError),
            Some(OperatorDisarmSignal::OrderEndpointDecodeError),
        ),
    ]
}

pub fn http_status_outcome_matrix() -> Vec<GatewayRealOrderEndpointHttpStatusOutcomeEntry> {
    let mut matrix = place_http_status_outcome_matrix();
    matrix.extend(cancel_http_status_outcome_matrix());
    matrix
}

fn documented_place_finam_rest_statuses() -> [u16; 4] {
    [200, 400, 401, 429]
}

fn documented_cancel_finam_rest_statuses() -> [u16; 5] {
    [200, 400, 401, 404, 429]
}

// A matrix row is intentionally explicit for implementation-gate review. This
// records current REST-doc evidence and defensive/waiver policies without
// enabling any real endpoint call.
#[allow(clippy::too_many_arguments)]
fn finam_status_semantics_entry(
    operation: GatewayRealOrderEndpointOperation,
    status_code: u16,
    documented_by_finam_rest_docs: bool,
    defensive_policy_only: bool,
    waiver_required_before_live: bool,
    body_policy: GatewayRealOrderEndpointFinamStatusBodyPolicy,
    future_send_outcome: GatewayRealOrderEndpointFutureSendOutcome,
    order_path_event: OrderPathEvent,
    order_path_state: OrderPathState,
    ack_status: CommandAckStatus,
    ack_reason_code: Option<CommandAckReasonCode>,
    operator_disarm_signal: Option<OperatorDisarmSignal>,
    body_required_for_immediate_submitted: bool,
    body_optional_for_cancel_acceptance: bool,
    empty_body_reconciliation_required: bool,
    cancel_reconciliation_required: bool,
) -> GatewayRealOrderEndpointFinamStatusSemanticsEntry {
    GatewayRealOrderEndpointFinamStatusSemanticsEntry {
        operation,
        status_code,
        documented_by_finam_rest_docs,
        defensive_policy_only,
        implementation_gate_evidence_required: !documented_by_finam_rest_docs
            || waiver_required_before_live,
        waiver_required_before_live,
        body_policy,
        future_send_outcome,
        order_path_event,
        order_path_state,
        ack_status,
        ack_reason_code,
        operator_disarm_signal,
        body_required_for_immediate_submitted,
        body_optional_for_cancel_acceptance,
        empty_body_reconciliation_required,
        cancel_reconciliation_required,
        no_blind_retry: true,
        raw_path_exported: false,
        raw_body_exported: false,
        raw_error_exported: false,
        raw_broker_order_id_exported: false,
        state_machine_transition_required: true,
    }
}

pub fn place_finam_status_semantics_matrix(
) -> Vec<GatewayRealOrderEndpointFinamStatusSemanticsEntry> {
    use GatewayRealOrderEndpointFinamStatusBodyPolicy as BodyPolicy;
    use GatewayRealOrderEndpointFutureSendOutcome as Outcome;

    vec![
        finam_status_semantics_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            200,
            true,
            false,
            false,
            BodyPolicy::PlaceSuccessBodyRequiredForSubmitted,
            Outcome::Accepted,
            OrderPathEvent::SubmitAccepted,
            OrderPathState::Submitted,
            CommandAckStatus::Submitted,
            None,
            None,
            true,
            false,
            false,
            false,
        ),
        finam_status_semantics_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            201,
            false,
            true,
            true,
            BodyPolicy::Undocumented2xxRequiresEvidenceOrWaiver,
            Outcome::DecodeError,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::ResponseDecodeError),
            Some(OperatorDisarmSignal::OrderEndpointDecodeError),
            false,
            false,
            true,
            false,
        ),
        finam_status_semantics_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            202,
            false,
            true,
            true,
            BodyPolicy::Undocumented2xxRequiresEvidenceOrWaiver,
            Outcome::DecodeError,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::ResponseDecodeError),
            Some(OperatorDisarmSignal::OrderEndpointDecodeError),
            false,
            false,
            true,
            false,
        ),
        finam_status_semantics_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            204,
            false,
            true,
            true,
            BodyPolicy::Undocumented2xxRequiresEvidenceOrWaiver,
            Outcome::DecodeError,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::ResponseDecodeError),
            Some(OperatorDisarmSignal::OrderEndpointDecodeError),
            false,
            false,
            true,
            false,
        ),
        finam_status_semantics_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            400,
            true,
            false,
            false,
            BodyPolicy::BrokerRejectBodyRedacted,
            Outcome::Rejected,
            OrderPathEvent::BrokerReject,
            OrderPathState::BrokerRejected,
            CommandAckStatus::Rejected,
            Some(CommandAckReasonCode::BrokerRejected),
            None,
            false,
            false,
            false,
            false,
        ),
        finam_status_semantics_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            401,
            true,
            false,
            false,
            BodyPolicy::UnauthorizedBodyRedacted,
            Outcome::Unauthorized,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::Unauthorized),
            Some(OperatorDisarmSignal::OrderEndpointUnauthorized),
            false,
            false,
            false,
            false,
        ),
        finam_status_semantics_entry(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            429,
            true,
            false,
            false,
            BodyPolicy::RateLimitBodyRedacted,
            Outcome::RateLimited,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::RateLimited),
            Some(OperatorDisarmSignal::OrderEndpointRateLimited),
            false,
            false,
            false,
            false,
        ),
    ]
}

pub fn cancel_finam_status_semantics_matrix(
) -> Vec<GatewayRealOrderEndpointFinamStatusSemanticsEntry> {
    use GatewayRealOrderEndpointFinamStatusBodyPolicy as BodyPolicy;
    use GatewayRealOrderEndpointFutureSendOutcome as Outcome;

    vec![
        finam_status_semantics_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            200,
            true,
            false,
            false,
            BodyPolicy::CancelSuccessBodyOptional,
            Outcome::Accepted,
            OrderPathEvent::CancelAccepted,
            OrderPathState::CancelSubmitted,
            CommandAckStatus::Submitted,
            None,
            None,
            false,
            true,
            false,
            false,
        ),
        finam_status_semantics_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            201,
            false,
            true,
            true,
            BodyPolicy::Undocumented2xxRequiresEvidenceOrWaiver,
            Outcome::DecodeError,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::ResponseDecodeError),
            Some(OperatorDisarmSignal::OrderEndpointDecodeError),
            false,
            false,
            true,
            true,
        ),
        finam_status_semantics_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            202,
            false,
            true,
            true,
            BodyPolicy::Undocumented2xxRequiresEvidenceOrWaiver,
            Outcome::DecodeError,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::ResponseDecodeError),
            Some(OperatorDisarmSignal::OrderEndpointDecodeError),
            false,
            false,
            true,
            true,
        ),
        finam_status_semantics_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            204,
            false,
            true,
            true,
            BodyPolicy::Undocumented2xxRequiresEvidenceOrWaiver,
            Outcome::DecodeError,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::ResponseDecodeError),
            Some(OperatorDisarmSignal::OrderEndpointDecodeError),
            false,
            false,
            true,
            true,
        ),
        finam_status_semantics_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            400,
            true,
            false,
            false,
            BodyPolicy::BrokerRejectBodyRedacted,
            Outcome::Rejected,
            OrderPathEvent::CancelRejected,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Rejected,
            Some(CommandAckReasonCode::BrokerRejected),
            None,
            false,
            false,
            false,
            false,
        ),
        finam_status_semantics_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            401,
            true,
            false,
            false,
            BodyPolicy::UnauthorizedBodyRedacted,
            Outcome::Unauthorized,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::Unauthorized),
            Some(OperatorDisarmSignal::OrderEndpointUnauthorized),
            false,
            false,
            false,
            false,
        ),
        finam_status_semantics_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            404,
            true,
            false,
            false,
            BodyPolicy::NotFoundRequiresReadOnlyReconciliation,
            Outcome::TimeoutUnknownPending,
            OrderPathEvent::CancelTimedOut,
            OrderPathState::CancelTimeoutUnknownPending,
            CommandAckStatus::UnknownPending,
            Some(CommandAckReasonCode::ReconciliationRequired),
            Some(OperatorDisarmSignal::CancelTimeoutUnknownPending),
            false,
            false,
            false,
            true,
        ),
        finam_status_semantics_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            409,
            false,
            true,
            true,
            BodyPolicy::NotFoundRequiresReadOnlyReconciliation,
            Outcome::TimeoutUnknownPending,
            OrderPathEvent::CancelTimedOut,
            OrderPathState::CancelTimeoutUnknownPending,
            CommandAckStatus::UnknownPending,
            Some(CommandAckReasonCode::ReconciliationRequired),
            Some(OperatorDisarmSignal::CancelTimeoutUnknownPending),
            false,
            false,
            false,
            true,
        ),
        finam_status_semantics_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            410,
            false,
            true,
            true,
            BodyPolicy::NotFoundRequiresReadOnlyReconciliation,
            Outcome::TimeoutUnknownPending,
            OrderPathEvent::CancelTimedOut,
            OrderPathState::CancelTimeoutUnknownPending,
            CommandAckStatus::UnknownPending,
            Some(CommandAckReasonCode::ReconciliationRequired),
            Some(OperatorDisarmSignal::CancelTimeoutUnknownPending),
            false,
            false,
            false,
            true,
        ),
        finam_status_semantics_entry(
            GatewayRealOrderEndpointOperation::CancelOrder,
            429,
            true,
            false,
            false,
            BodyPolicy::RateLimitBodyRedacted,
            Outcome::RateLimited,
            OrderPathEvent::RequireManualIntervention,
            OrderPathState::ManualInterventionRequired,
            CommandAckStatus::Error,
            Some(CommandAckReasonCode::RateLimited),
            Some(OperatorDisarmSignal::OrderEndpointRateLimited),
            false,
            false,
            false,
            false,
        ),
    ]
}

pub fn finam_status_semantics_matrix() -> Vec<GatewayRealOrderEndpointFinamStatusSemanticsEntry> {
    let mut matrix = place_finam_status_semantics_matrix();
    matrix.extend(cancel_finam_status_semantics_matrix());
    matrix
}

fn evidence_slot_entry(
    slot: GatewayRealOrderEndpointEvidenceSlot,
    accepted_closure_methods: Vec<GatewayRealOrderEndpointEvidenceClosureMethod>,
    order_endpoint_calls_allowed_for_closure: bool,
) -> GatewayRealOrderEndpointEvidenceSlotClosurePlanEntry {
    GatewayRealOrderEndpointEvidenceSlotClosurePlanEntry {
        slot,
        current_status:
            GatewayRealOrderEndpointEvidenceClosureStatus::PendingBeforeImplementationGate,
        accepted_closure_methods,
        must_close_before_implementation_gate: true,
        endpoint_calls_allowed_for_closure: false,
        order_endpoint_calls_allowed_for_closure,
        reviewer_acceptance_required: true,
        artifact_redacted_only: true,
        source_archive_binding_required: true,
        raw_secret_exported: false,
        raw_account_exported: false,
        raw_order_id_exported: false,
        raw_path_exported: false,
        raw_body_exported: false,
    }
}

pub fn implementation_gate_evidence_closure_plan(
) -> Vec<GatewayRealOrderEndpointEvidenceSlotClosurePlanEntry> {
    use GatewayRealOrderEndpointEvidenceClosureMethod as Method;
    use GatewayRealOrderEndpointEvidenceSlot as Slot;

    vec![
        evidence_slot_entry(
            Slot::ReleaseProfileEvidenceOrWaiver,
            vec![Method::ControlledEvidence, Method::ReviewerAcceptedWaiver],
            false,
        ),
        evidence_slot_entry(
            Slot::PositiveGetOrderEvidenceOrWaiver,
            vec![Method::ControlledEvidence, Method::ReviewerAcceptedWaiver],
            false,
        ),
        evidence_slot_entry(
            Slot::RouteTemplateRecheck,
            vec![
                Method::OfficialDocsConfirmation,
                Method::ReviewerAcceptedWaiver,
            ],
            false,
        ),
        evidence_slot_entry(
            Slot::Undocumented2xxStatusSemantics,
            vec![
                Method::ControlledEvidence,
                Method::OfficialDocsConfirmation,
                Method::ReviewerAcceptedWaiver,
            ],
            false,
        ),
        evidence_slot_entry(
            Slot::Cancel409410StatusSemantics,
            vec![
                Method::ControlledEvidence,
                Method::OfficialDocsConfirmation,
                Method::ReviewerAcceptedWaiver,
            ],
            false,
        ),
    ]
}

fn durable_attempt_journal_column(
    name: &str,
    kind: GatewayRealOrderEndpointDurableAttemptJournalColumnKind,
    required: bool,
    unique: bool,
) -> GatewayRealOrderEndpointDurableAttemptJournalColumn {
    GatewayRealOrderEndpointDurableAttemptJournalColumn {
        name: name.to_string(),
        kind,
        required,
        unique,
        stores_raw_value: false,
    }
}

pub fn durable_attempt_journal_sqlite_columns(
) -> Vec<GatewayRealOrderEndpointDurableAttemptJournalColumn> {
    use GatewayRealOrderEndpointDurableAttemptJournalColumnKind as Kind;

    vec![
        durable_attempt_journal_column(
            "endpoint_attempt_id_sha256",
            Kind::PrimaryKeyHash,
            true,
            true,
        ),
        durable_attempt_journal_column("request_id_sha256", Kind::ForeignKeyHash, true, false),
        durable_attempt_journal_column("client_order_id_sha256", Kind::Sha256, true, false),
        durable_attempt_journal_column("account_sha256", Kind::Sha256, true, false),
        durable_attempt_journal_column("instrument_sha256", Kind::Sha256, true, false),
        durable_attempt_journal_column("broker_order_id_sha256", Kind::Sha256, false, false),
        durable_attempt_journal_column("operation", Kind::SafeEnum, true, false),
        durable_attempt_journal_column("checkpoint_label", Kind::SafeEnum, true, false),
        durable_attempt_journal_column("request_fingerprint_sha256", Kind::Sha256, true, false),
        durable_attempt_journal_column("checkpoint_proof_sha256", Kind::Sha256, true, false),
        durable_attempt_journal_column("captured_envelope_sha256", Kind::Sha256, true, false),
        durable_attempt_journal_column("outcome_sha256", Kind::Sha256, true, false),
        durable_attempt_journal_column("state_transition_sha256", Kind::Sha256, true, false),
        durable_attempt_journal_column("ack_diagnostic_sha256", Kind::Sha256, true, false),
        durable_attempt_journal_column("http_status_present", Kind::Boolean, true, false),
        durable_attempt_journal_column("http_status_code", Kind::Integer, false, false),
        durable_attempt_journal_column("created_ts", Kind::Timestamp, true, false),
        durable_attempt_journal_column("state_transition_committed", Kind::Boolean, true, false),
        durable_attempt_journal_column("replay_fingerprint_set_sha256", Kind::Sha256, true, false),
    ]
}

fn durable_attempt_journal_index(
    name: &str,
    columns: &[&str],
    unique: bool,
    replay_policy_related: bool,
) -> GatewayRealOrderEndpointDurableAttemptJournalIndex {
    GatewayRealOrderEndpointDurableAttemptJournalIndex {
        name: name.to_string(),
        columns: columns.iter().map(|column| (*column).to_string()).collect(),
        unique,
        replay_policy_related,
    }
}

pub fn durable_attempt_journal_sqlite_indexes(
) -> Vec<GatewayRealOrderEndpointDurableAttemptJournalIndex> {
    vec![
        durable_attempt_journal_index(
            "ux_order_endpoint_attempts_attempt_id",
            &["endpoint_attempt_id_sha256"],
            true,
            true,
        ),
        durable_attempt_journal_index(
            "ix_order_endpoint_attempts_request_id",
            &["request_id_sha256"],
            false,
            false,
        ),
        durable_attempt_journal_index(
            "ix_order_endpoint_attempts_client_order_id",
            &["client_order_id_sha256"],
            false,
            false,
        ),
        durable_attempt_journal_index(
            "ix_order_endpoint_attempts_replay_set",
            &[
                "endpoint_attempt_id_sha256",
                "replay_fingerprint_set_sha256",
            ],
            false,
            true,
        ),
    ]
}

pub fn durable_attempt_journal_replay_policy_matrix(
) -> Vec<GatewayRealOrderEndpointDurableAttemptJournalReplayPolicyEntry> {
    use GatewayRealOrderEndpointDurableAttemptJournalReplayDecision as Decision;

    vec![
        GatewayRealOrderEndpointDurableAttemptJournalReplayPolicyEntry {
            decision: Decision::SameFingerprintSetIdempotentReplay,
            endpoint_attempt_id_hash_match_required: true,
            full_fingerprint_set_match_required: true,
            state_transition_result_match_required: true,
            ack_diagnostic_match_required: true,
            raw_values_compared: false,
            no_blind_retry: true,
            operator_disarm_on_conflict: false,
        },
        GatewayRealOrderEndpointDurableAttemptJournalReplayPolicyEntry {
            decision: Decision::DifferentFingerprintSetRejectAndDisarm,
            endpoint_attempt_id_hash_match_required: true,
            full_fingerprint_set_match_required: false,
            state_transition_result_match_required: false,
            ack_diagnostic_match_required: false,
            raw_values_compared: false,
            no_blind_retry: true,
            operator_disarm_on_conflict: true,
        },
    ]
}

fn durable_journal_migration_step(
    step: GatewayRealOrderEndpointDurableJournalMigrationStepKind,
) -> GatewayRealOrderEndpointDurableJournalMigrationStep {
    GatewayRealOrderEndpointDurableJournalMigrationStep {
        step,
        required_before_endpoint_gate: true,
        operator_visible: true,
        failure_disarms_order_endpoints: true,
        raw_values_exported: false,
    }
}

pub fn durable_journal_migration_runbook_steps(
) -> Vec<GatewayRealOrderEndpointDurableJournalMigrationStep> {
    use GatewayRealOrderEndpointDurableJournalMigrationStepKind as Step;

    vec![
        durable_journal_migration_step(Step::BackupBeforeMigration),
        durable_journal_migration_step(Step::AcquireSingleWriterLock),
        durable_journal_migration_step(Step::OpenSqliteWithWal),
        durable_journal_migration_step(Step::SetSynchronousFull),
        durable_journal_migration_step(Step::VerifySchemaVersion),
        durable_journal_migration_step(Step::CreateOrderEndpointAttemptsTable),
        durable_journal_migration_step(Step::CreateReplayIndexes),
        durable_journal_migration_step(Step::RunIntegrityCheck),
        durable_journal_migration_step(Step::RefuseAutoRepair),
    ]
}

fn canonical_replay_fingerprint_field(
    ordinal: usize,
    field: GatewayRealOrderEndpointCanonicalReplayFingerprintField,
) -> GatewayRealOrderEndpointCanonicalReplayFingerprintFieldEntry {
    GatewayRealOrderEndpointCanonicalReplayFingerprintFieldEntry {
        ordinal,
        field,
        required: true,
        hash_len: 64,
        raw_value_exported: false,
    }
}

pub fn canonical_replay_fingerprint_fields(
) -> Vec<GatewayRealOrderEndpointCanonicalReplayFingerprintFieldEntry> {
    use GatewayRealOrderEndpointCanonicalReplayFingerprintField as Field;

    [
        Field::SchemaVersion,
        Field::Operation,
        Field::EndpointAttemptIdSha256,
        Field::RequestIdSha256,
        Field::ClientOrderIdSha256,
        Field::AccountSha256,
        Field::InstrumentSha256,
        Field::CheckpointLabel,
        Field::RequestFingerprintSha256,
        Field::CheckpointProofSha256,
        Field::CapturedEnvelopeSha256,
        Field::OutcomeSha256,
        Field::StateTransitionSha256,
        Field::AckDiagnosticSha256,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, field)| canonical_replay_fingerprint_field(index + 1, field))
    .collect()
}

fn endpoint_attempt_id_lifecycle_entry(
    phase: GatewayRealOrderEndpointAttemptIdLifecyclePhase,
    new_network_attempt_allowed: bool,
) -> GatewayRealOrderEndpointAttemptIdLifecyclePolicyEntry {
    GatewayRealOrderEndpointAttemptIdLifecyclePolicyEntry {
        phase,
        requires_endpoint_gate: true,
        requires_request_id_hash: true,
        requires_client_order_id_hash: true,
        requires_operation: true,
        endpoint_attempt_id_sha256_len: 64,
        new_network_attempt_allowed,
        raw_endpoint_attempt_id_exported: false,
    }
}

pub fn endpoint_attempt_id_lifecycle_policy(
) -> Vec<GatewayRealOrderEndpointAttemptIdLifecyclePolicyEntry> {
    use GatewayRealOrderEndpointAttemptIdLifecyclePhase as Phase;

    vec![
        endpoint_attempt_id_lifecycle_entry(Phase::GeneratedAfterApprovedRequestParts, true),
        endpoint_attempt_id_lifecycle_entry(Phase::BoundBeforeEndpointSend, true),
        endpoint_attempt_id_lifecycle_entry(Phase::PersistedWithAttemptJournal, true),
        endpoint_attempt_id_lifecycle_entry(Phase::ReusedOnlyForIdempotentReplay, false),
        endpoint_attempt_id_lifecycle_entry(
            Phase::NeverReusedForNewAttemptAfterTerminalOrManual,
            false,
        ),
    ]
}

fn readiness_checklist_entry(
    item: GatewayRealOrderEndpointImplementationGateReadinessItem,
    status: GatewayRealOrderEndpointImplementationGateReadinessStatus,
) -> GatewayRealOrderEndpointImplementationGateReadinessChecklistEntry {
    GatewayRealOrderEndpointImplementationGateReadinessChecklistEntry {
        item,
        status,
        must_close_before_implementation_gate: true,
        reviewer_acceptance_required: true,
        endpoint_calls_allowed_for_check: false,
        raw_values_exported: false,
    }
}

pub fn implementation_gate_readiness_checklist(
) -> Vec<GatewayRealOrderEndpointImplementationGateReadinessChecklistEntry> {
    use GatewayRealOrderEndpointImplementationGateReadinessItem as Item;
    use GatewayRealOrderEndpointImplementationGateReadinessStatus as Status;

    vec![
        readiness_checklist_entry(Item::ForbiddenSurfaceScanners, Status::ImplementedAndTested),
        readiness_checklist_entry(
            Item::EndpointGateUnconstructible,
            Status::ImplementedAndTested,
        ),
        readiness_checklist_entry(
            Item::DurableStoreImplementedTested,
            Status::ImplementedAndTested,
        ),
        readiness_checklist_entry(
            Item::OperatorArmImplementedTested,
            Status::ImplementedAndTested,
        ),
        readiness_checklist_entry(
            Item::RateLimitBackoffImplementedTested,
            Status::ImplementedAndTested,
        ),
        readiness_checklist_entry(
            Item::NoBlindRetryImplementedTested,
            Status::ImplementedAndTested,
        ),
        readiness_checklist_entry(
            Item::RedactedAckImplementedTested,
            Status::ImplementedAndTested,
        ),
        readiness_checklist_entry(
            Item::ReleaseProfileEvidenceOrWaiver,
            Status::PendingEvidenceOrWaiver,
        ),
        readiness_checklist_entry(
            Item::PositiveGetOrderEvidenceOrWaiver,
            Status::PendingEvidenceOrWaiver,
        ),
        readiness_checklist_entry(Item::RouteTemplateRecheck, Status::PendingEvidenceOrWaiver),
        readiness_checklist_entry(Item::CanonicalReplayGoldenVectors, Status::DesignRecorded),
        readiness_checklist_entry(Item::OperatorReplayRunbook, Status::DesignRecorded),
    ]
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut sha256 = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut sha256, "{byte:02x}").expect("hex write cannot fail");
    }
    sha256
}

pub fn canonical_replay_golden_vectors() -> Vec<GatewayRealOrderEndpointCanonicalReplayGoldenVector>
{
    let canonical_json = concat!(
        "{\"account_sha256\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",",
        "\"ack_diagnostic_sha256\":\"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\",",
        "\"captured_envelope_sha256\":\"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\",",
        "\"checkpoint_label\":\"PlaceBeginSubmitPersistedBeforeEndpoint\",",
        "\"checkpoint_proof_sha256\":\"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd\",",
        "\"client_order_id_sha256\":\"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee\",",
        "\"endpoint_attempt_id_sha256\":\"1111111111111111111111111111111111111111111111111111111111111111\",",
        "\"instrument_sha256\":\"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\",",
        "\"operation\":\"PlaceOrder\",",
        "\"outcome_sha256\":\"9999999999999999999999999999999999999999999999999999999999999999\",",
        "\"request_fingerprint_sha256\":\"7777777777777777777777777777777777777777777777777777777777777777\",",
        "\"request_id_sha256\":\"2222222222222222222222222222222222222222222222222222222222222222\",",
        "\"schema_version\":1,",
        "\"state_transition_sha256\":\"8888888888888888888888888888888888888888888888888888888888888888\"}"
    );
    let expected_sha256 = sha256_hex(canonical_json.as_bytes());
    debug_assert_eq!(
        expected_sha256,
        "d467afd3b7d320c26966a1a400995e00664397ed47bb74320a418cfd2524abc6"
    );

    vec![GatewayRealOrderEndpointCanonicalReplayGoldenVector {
        name: "place_order_schema_v1_sorted_keys_no_whitespace".to_string(),
        schema_version: 1,
        encoding:
            GatewayRealOrderEndpointCanonicalReplayEncoding::Utf8JsonObjectSortedKeysNoWhitespace,
        canonical_json: canonical_json.to_string(),
        expected_sha256,
        field_count: canonical_replay_fingerprint_fields().len(),
        no_whitespace: true,
        sorted_keys_required: true,
        raw_values_exported: false,
    }]
}

fn operator_replay_runbook_entry(
    case: GatewayRealOrderEndpointOperatorReplayRunbookCase,
    same_endpoint_attempt_id_allowed: bool,
    new_endpoint_attempt_id_required: bool,
    disarm_required: bool,
) -> GatewayRealOrderEndpointOperatorReplayRunbookEntry {
    GatewayRealOrderEndpointOperatorReplayRunbookEntry {
        case,
        operator_visible: true,
        redacted_diagnostic_only: true,
        same_endpoint_attempt_id_allowed,
        new_endpoint_attempt_id_required,
        disarm_required,
        no_blind_retry: true,
        raw_endpoint_attempt_id_exported: false,
    }
}

pub fn operator_replay_runbook_entries() -> Vec<GatewayRealOrderEndpointOperatorReplayRunbookEntry>
{
    use GatewayRealOrderEndpointOperatorReplayRunbookCase as Case;

    vec![
        operator_replay_runbook_entry(Case::IdempotentReplaySameFingerprint, true, false, false),
        operator_replay_runbook_entry(Case::ConflictingReplayDisarm, false, true, true),
        operator_replay_runbook_entry(Case::TimeoutUnknownPending, false, true, true),
        operator_replay_runbook_entry(Case::ManualIntervention, false, true, true),
        operator_replay_runbook_entry(Case::TerminalOutcomeNewAttempt, false, true, false),
    ]
}

fn evidence_closure_package_entry(
    slot: GatewayRealOrderEndpointEvidenceSlot,
) -> GatewayRealOrderEndpointEvidenceClosurePackageEntry {
    GatewayRealOrderEndpointEvidenceClosurePackageEntry {
        slot,
        status: GatewayRealOrderEndpointEvidenceClosurePackageStatus::PendingEvidenceOrWaiver,
        evidence_or_waiver_required: true,
        reviewer_acceptance_required: true,
        source_archive_binding_required: true,
        order_endpoint_calls_allowed_for_closure: false,
        raw_values_exported: false,
    }
}

pub fn evidence_closure_package_entries() -> Vec<GatewayRealOrderEndpointEvidenceClosurePackageEntry>
{
    use GatewayRealOrderEndpointEvidenceSlot as Slot;

    vec![
        evidence_closure_package_entry(Slot::ReleaseProfileEvidenceOrWaiver),
        evidence_closure_package_entry(Slot::PositiveGetOrderEvidenceOrWaiver),
        evidence_closure_package_entry(Slot::RouteTemplateRecheck),
        evidence_closure_package_entry(Slot::Undocumented2xxStatusSemantics),
        evidence_closure_package_entry(Slot::Cancel409410StatusSemantics),
    ]
}

pub fn future_send_outcome_state_policy_matrix(
) -> Vec<GatewayRealOrderEndpointOutcomeStatePolicyEntry> {
    use GatewayRealOrderEndpointFutureSendOutcome as Outcome;
    use GatewayRealOrderEndpointOperatorPolicy as OperatorPolicy;

    vec![
        GatewayRealOrderEndpointOutcomeStatePolicyEntry {
            outcome: Outcome::Accepted,
            place_event: OrderPathEvent::SubmitAccepted,
            place_state: OrderPathState::Submitted,
            cancel_event: OrderPathEvent::CancelAccepted,
            cancel_state: OrderPathState::CancelSubmitted,
            error_kind: None,
            ack_status: CommandAckStatus::Submitted,
            place_ack_reason_code: None,
            cancel_ack_reason_code: None,
            operator_policy: OperatorPolicy::None,
            operator_disarm_signal: None,
            backoff_required: false,
            manual_intervention_required: false,
            no_blind_retry: false,
            state_machine_transition_required: true,
            result_diagnostic_can_bypass_state_machine: false,
            runtime_ack_redacted_only: true,
        },
        GatewayRealOrderEndpointOutcomeStatePolicyEntry {
            outcome: Outcome::Rejected,
            place_event: OrderPathEvent::BrokerReject,
            place_state: OrderPathState::BrokerRejected,
            cancel_event: OrderPathEvent::CancelRejected,
            cancel_state: OrderPathState::ManualInterventionRequired,
            error_kind: Some(OrderPathErrorKind::BrokerRejected),
            ack_status: CommandAckStatus::Rejected,
            place_ack_reason_code: Some(CommandAckReasonCode::BrokerRejected),
            cancel_ack_reason_code: Some(CommandAckReasonCode::BrokerRejected),
            operator_policy: OperatorPolicy::None,
            operator_disarm_signal: None,
            backoff_required: false,
            manual_intervention_required: false,
            no_blind_retry: false,
            state_machine_transition_required: true,
            result_diagnostic_can_bypass_state_machine: false,
            runtime_ack_redacted_only: true,
        },
        GatewayRealOrderEndpointOutcomeStatePolicyEntry {
            outcome: Outcome::TimeoutUnknownPending,
            place_event: OrderPathEvent::SubmitTimedOut,
            place_state: OrderPathState::TimeoutUnknownPending,
            cancel_event: OrderPathEvent::CancelTimedOut,
            cancel_state: OrderPathState::CancelTimeoutUnknownPending,
            error_kind: Some(OrderPathErrorKind::TransportTimeout),
            ack_status: CommandAckStatus::Timeout,
            place_ack_reason_code: Some(CommandAckReasonCode::TimeoutUnknownPending),
            cancel_ack_reason_code: Some(CommandAckReasonCode::CancelTimeoutUnknownPending),
            operator_policy: OperatorPolicy::DisarmAndOperatorIntervention,
            operator_disarm_signal: Some(OperatorDisarmSignal::UnknownPendingOrder),
            backoff_required: false,
            manual_intervention_required: true,
            no_blind_retry: true,
            state_machine_transition_required: true,
            result_diagnostic_can_bypass_state_machine: false,
            runtime_ack_redacted_only: true,
        },
        GatewayRealOrderEndpointOutcomeStatePolicyEntry {
            outcome: Outcome::RateLimited,
            place_event: OrderPathEvent::RequireManualIntervention,
            place_state: OrderPathState::ManualInterventionRequired,
            cancel_event: OrderPathEvent::RequireManualIntervention,
            cancel_state: OrderPathState::ManualInterventionRequired,
            error_kind: Some(OrderPathErrorKind::RateLimited),
            ack_status: CommandAckStatus::Error,
            place_ack_reason_code: Some(CommandAckReasonCode::RateLimited),
            cancel_ack_reason_code: Some(CommandAckReasonCode::RateLimited),
            operator_policy: OperatorPolicy::BackoffAndManualIntervention,
            operator_disarm_signal: Some(OperatorDisarmSignal::OrderEndpointRateLimited),
            backoff_required: true,
            manual_intervention_required: true,
            no_blind_retry: true,
            state_machine_transition_required: true,
            result_diagnostic_can_bypass_state_machine: false,
            runtime_ack_redacted_only: true,
        },
        GatewayRealOrderEndpointOutcomeStatePolicyEntry {
            outcome: Outcome::Maintenance,
            place_event: OrderPathEvent::RequireManualIntervention,
            place_state: OrderPathState::ManualInterventionRequired,
            cancel_event: OrderPathEvent::RequireManualIntervention,
            cancel_state: OrderPathState::ManualInterventionRequired,
            error_kind: Some(OrderPathErrorKind::BrokerMaintenance),
            ack_status: CommandAckStatus::Error,
            place_ack_reason_code: Some(CommandAckReasonCode::BrokerMaintenance),
            cancel_ack_reason_code: Some(CommandAckReasonCode::BrokerMaintenance),
            operator_policy: OperatorPolicy::DegradeAndManualIntervention,
            operator_disarm_signal: Some(OperatorDisarmSignal::OrderEndpointMaintenance),
            backoff_required: false,
            manual_intervention_required: true,
            no_blind_retry: true,
            state_machine_transition_required: true,
            result_diagnostic_can_bypass_state_machine: false,
            runtime_ack_redacted_only: true,
        },
        GatewayRealOrderEndpointOutcomeStatePolicyEntry {
            outcome: Outcome::Unauthorized,
            place_event: OrderPathEvent::RequireManualIntervention,
            place_state: OrderPathState::ManualInterventionRequired,
            cancel_event: OrderPathEvent::RequireManualIntervention,
            cancel_state: OrderPathState::ManualInterventionRequired,
            error_kind: Some(OrderPathErrorKind::Unauthorized),
            ack_status: CommandAckStatus::Error,
            place_ack_reason_code: Some(CommandAckReasonCode::Unauthorized),
            cancel_ack_reason_code: Some(CommandAckReasonCode::Unauthorized),
            operator_policy: OperatorPolicy::DisarmAndOperatorIntervention,
            operator_disarm_signal: Some(OperatorDisarmSignal::OrderEndpointUnauthorized),
            backoff_required: false,
            manual_intervention_required: true,
            no_blind_retry: true,
            state_machine_transition_required: true,
            result_diagnostic_can_bypass_state_machine: false,
            runtime_ack_redacted_only: true,
        },
        GatewayRealOrderEndpointOutcomeStatePolicyEntry {
            outcome: Outcome::DecodeError,
            place_event: OrderPathEvent::RequireManualIntervention,
            place_state: OrderPathState::ManualInterventionRequired,
            cancel_event: OrderPathEvent::RequireManualIntervention,
            cancel_state: OrderPathState::ManualInterventionRequired,
            error_kind: Some(OrderPathErrorKind::ResponseDecodeError),
            ack_status: CommandAckStatus::Error,
            place_ack_reason_code: Some(CommandAckReasonCode::ResponseDecodeError),
            cancel_ack_reason_code: Some(CommandAckReasonCode::ResponseDecodeError),
            operator_policy: OperatorPolicy::DecodeManualIntervention,
            operator_disarm_signal: Some(OperatorDisarmSignal::OrderEndpointDecodeError),
            backoff_required: false,
            manual_intervention_required: true,
            no_blind_retry: true,
            state_machine_transition_required: true,
            result_diagnostic_can_bypass_state_machine: false,
            runtime_ack_redacted_only: true,
        },
        GatewayRealOrderEndpointOutcomeStatePolicyEntry {
            outcome: Outcome::TransportError,
            place_event: OrderPathEvent::RequireManualIntervention,
            place_state: OrderPathState::ManualInterventionRequired,
            cancel_event: OrderPathEvent::RequireManualIntervention,
            cancel_state: OrderPathState::ManualInterventionRequired,
            error_kind: Some(OrderPathErrorKind::Unknown),
            ack_status: CommandAckStatus::Error,
            place_ack_reason_code: Some(CommandAckReasonCode::ManualInterventionRequired),
            cancel_ack_reason_code: Some(CommandAckReasonCode::ManualInterventionRequired),
            operator_policy: OperatorPolicy::TransportCategoryManualIntervention,
            operator_disarm_signal: Some(OperatorDisarmSignal::GatewayDegraded),
            backoff_required: true,
            manual_intervention_required: true,
            no_blind_retry: true,
            state_machine_transition_required: true,
            result_diagnostic_can_bypass_state_machine: false,
            runtime_ack_redacted_only: true,
        },
    ]
}

pub fn accepted_broker_id_policy_matrix() -> Vec<GatewayRealOrderEndpointAcceptedBrokerIdPolicyEntry>
{
    use GatewayRealOrderEndpointAcceptedBrokerIdPolicy as Policy;

    vec![
        GatewayRealOrderEndpointAcceptedBrokerIdPolicyEntry {
            policy: Policy::AcceptedWithBrokerOrderId,
            place_event: OrderPathEvent::SubmitAccepted,
            place_state: OrderPathState::Submitted,
            ack_status: CommandAckStatus::Submitted,
            ack_reason_code: None,
            operator_disarm_signal: None,
            reconciliation_required: false,
            no_blind_retry: false,
            manual_intervention_required: false,
            raw_broker_order_id_exported: false,
            runtime_ack_redacted_only: true,
        },
        GatewayRealOrderEndpointAcceptedBrokerIdPolicyEntry {
            policy: Policy::AcceptedWithoutBrokerOrderId,
            place_event: OrderPathEvent::SubmitAcceptedWithoutBrokerOrderId,
            place_state: OrderPathState::SubmittedPendingBrokerOrderId,
            ack_status: CommandAckStatus::UnknownPending,
            ack_reason_code: Some(CommandAckReasonCode::ReconciliationRequired),
            operator_disarm_signal: Some(OperatorDisarmSignal::AcceptedWithoutBrokerOrderId),
            reconciliation_required: true,
            no_blind_retry: true,
            manual_intervention_required: true,
            raw_broker_order_id_exported: false,
            runtime_ack_redacted_only: true,
        },
        GatewayRealOrderEndpointAcceptedBrokerIdPolicyEntry {
            policy: Policy::EmptyBrokerOrderIdDecodeError,
            place_event: OrderPathEvent::RequireManualIntervention,
            place_state: OrderPathState::ManualInterventionRequired,
            ack_status: CommandAckStatus::Error,
            ack_reason_code: Some(CommandAckReasonCode::ResponseDecodeError),
            operator_disarm_signal: Some(OperatorDisarmSignal::OrderEndpointDecodeError),
            reconciliation_required: true,
            no_blind_retry: true,
            manual_intervention_required: true,
            raw_broker_order_id_exported: false,
            runtime_ack_redacted_only: true,
        },
        GatewayRealOrderEndpointAcceptedBrokerIdPolicyEntry {
            policy: Policy::BrokerOrderIdMismatchManualIntervention,
            place_event: OrderPathEvent::RequireManualIntervention,
            place_state: OrderPathState::ManualInterventionRequired,
            ack_status: CommandAckStatus::UnknownPending,
            ack_reason_code: Some(CommandAckReasonCode::ManualInterventionRequired),
            operator_disarm_signal: Some(OperatorDisarmSignal::ReconciliationConflict),
            reconciliation_required: true,
            no_blind_retry: true,
            manual_intervention_required: true,
            raw_broker_order_id_exported: false,
            runtime_ack_redacted_only: true,
        },
    ]
}

pub fn accepted_result_classifier_policy_matrix(
) -> Vec<GatewayRealOrderEndpointAcceptedResultPolicyEntry> {
    use GatewayRealOrderEndpointAcceptedBrokerIdPolicy as Policy;
    use GatewayRealOrderEndpointAcceptedResultKind as Kind;
    use GatewayRealOrderEndpointFutureSendOutcome as Outcome;

    vec![
        GatewayRealOrderEndpointAcceptedResultPolicyEntry {
            kind: Kind::WithBrokerOrderId,
            inherited_policy: Policy::AcceptedWithBrokerOrderId,
            send_outcome: Outcome::Accepted,
            place_event: OrderPathEvent::SubmitAccepted,
            place_state: OrderPathState::Submitted,
            ack_status: CommandAckStatus::Submitted,
            ack_reason_code: None,
            operator_disarm_signal: None,
            reconciliation_required: false,
            no_blind_retry: false,
            manual_intervention_required: false,
            raw_broker_order_id_exported: false,
            runtime_ack_redacted_only: true,
            state_machine_transition_required: true,
            accepted_result_can_be_treated_as_unconditional_submitted: false,
        },
        GatewayRealOrderEndpointAcceptedResultPolicyEntry {
            kind: Kind::WithoutBrokerOrderId,
            inherited_policy: Policy::AcceptedWithoutBrokerOrderId,
            send_outcome: Outcome::Accepted,
            place_event: OrderPathEvent::SubmitAcceptedWithoutBrokerOrderId,
            place_state: OrderPathState::SubmittedPendingBrokerOrderId,
            ack_status: CommandAckStatus::UnknownPending,
            ack_reason_code: Some(CommandAckReasonCode::ReconciliationRequired),
            operator_disarm_signal: Some(OperatorDisarmSignal::AcceptedWithoutBrokerOrderId),
            reconciliation_required: true,
            no_blind_retry: true,
            manual_intervention_required: true,
            raw_broker_order_id_exported: false,
            runtime_ack_redacted_only: true,
            state_machine_transition_required: true,
            accepted_result_can_be_treated_as_unconditional_submitted: false,
        },
        GatewayRealOrderEndpointAcceptedResultPolicyEntry {
            kind: Kind::EmptyBrokerOrderId,
            inherited_policy: Policy::EmptyBrokerOrderIdDecodeError,
            send_outcome: Outcome::Accepted,
            place_event: OrderPathEvent::RequireManualIntervention,
            place_state: OrderPathState::ManualInterventionRequired,
            ack_status: CommandAckStatus::Error,
            ack_reason_code: Some(CommandAckReasonCode::ResponseDecodeError),
            operator_disarm_signal: Some(OperatorDisarmSignal::OrderEndpointDecodeError),
            reconciliation_required: true,
            no_blind_retry: true,
            manual_intervention_required: true,
            raw_broker_order_id_exported: false,
            runtime_ack_redacted_only: true,
            state_machine_transition_required: true,
            accepted_result_can_be_treated_as_unconditional_submitted: false,
        },
        GatewayRealOrderEndpointAcceptedResultPolicyEntry {
            kind: Kind::BrokerOrderIdMismatch,
            inherited_policy: Policy::BrokerOrderIdMismatchManualIntervention,
            send_outcome: Outcome::Accepted,
            place_event: OrderPathEvent::RequireManualIntervention,
            place_state: OrderPathState::ManualInterventionRequired,
            ack_status: CommandAckStatus::UnknownPending,
            ack_reason_code: Some(CommandAckReasonCode::ManualInterventionRequired),
            operator_disarm_signal: Some(OperatorDisarmSignal::ReconciliationConflict),
            reconciliation_required: true,
            no_blind_retry: true,
            manual_intervention_required: true,
            raw_broker_order_id_exported: false,
            runtime_ack_redacted_only: true,
            state_machine_transition_required: true,
            accepted_result_can_be_treated_as_unconditional_submitted: false,
        },
    ]
}

pub fn cancel_accepted_id_policy_matrix() -> Vec<GatewayRealOrderEndpointCancelAcceptedIdPolicyEntry>
{
    use GatewayRealOrderEndpointCancelAcceptedIdPolicy as Policy;

    vec![
        GatewayRealOrderEndpointCancelAcceptedIdPolicyEntry {
            policy: Policy::MatchingBrokerOrderId,
            cancel_event: OrderPathEvent::CancelAccepted,
            cancel_state: OrderPathState::CancelSubmitted,
            ack_status: CommandAckStatus::Submitted,
            ack_reason_code: None,
            operator_disarm_signal: None,
            response_body_required: false,
            broker_order_id_required: false,
            reconciliation_required: false,
            no_blind_retry: true,
            manual_intervention_required: false,
            raw_broker_order_id_exported: false,
            runtime_ack_redacted_only: true,
        },
        GatewayRealOrderEndpointCancelAcceptedIdPolicyEntry {
            policy: Policy::MissingBrokerOrderIdAcceptedPendingReconciliation,
            cancel_event: OrderPathEvent::CancelAccepted,
            cancel_state: OrderPathState::CancelSubmitted,
            ack_status: CommandAckStatus::UnknownPending,
            ack_reason_code: Some(CommandAckReasonCode::ReconciliationRequired),
            operator_disarm_signal: Some(OperatorDisarmSignal::AcceptedWithoutBrokerOrderId),
            response_body_required: false,
            broker_order_id_required: false,
            reconciliation_required: true,
            no_blind_retry: true,
            manual_intervention_required: true,
            raw_broker_order_id_exported: false,
            runtime_ack_redacted_only: true,
        },
        GatewayRealOrderEndpointCancelAcceptedIdPolicyEntry {
            policy: Policy::BrokerOrderIdMismatchManualIntervention,
            cancel_event: OrderPathEvent::RequireManualIntervention,
            cancel_state: OrderPathState::ManualInterventionRequired,
            ack_status: CommandAckStatus::UnknownPending,
            ack_reason_code: Some(CommandAckReasonCode::ManualInterventionRequired),
            operator_disarm_signal: Some(OperatorDisarmSignal::CancelBrokerOrderIdMismatch),
            response_body_required: false,
            broker_order_id_required: false,
            reconciliation_required: true,
            no_blind_retry: true,
            manual_intervention_required: true,
            raw_broker_order_id_exported: false,
            runtime_ack_redacted_only: true,
        },
    ]
}

pub fn captured_envelope_transport_category_matrix(
) -> Vec<GatewayRealOrderEndpointCapturedEnvelopeTransportCategoryEntry> {
    transport_categories()
        .into_iter()
        .map(
            |category| GatewayRealOrderEndpointCapturedEnvelopeTransportCategoryEntry {
                category,
                envelope_kind: GatewayRealOrderEndpointCapturedEnvelopeKind::TransportError,
                raw_error_exported: false,
                transport_category_exported: true,
                error_len_recorded: true,
                error_sha256_recorded: true,
            },
        )
        .collect()
}

fn create_place_checkpoint_marker_after_sqlite_transition(
    _gate: &EndpointGateApproved,
    proof: &GatewayRealOrderEndpointSqliteTransitionCommitProof,
) -> Result<
    PlaceEndpointDurableCheckpointApproved,
    GatewayRealOrderEndpointCheckpointMarkerCreationError,
> {
    validate_checkpoint_marker_creation_proof(
        proof,
        OrderPathEvent::BeginSubmit,
        GatewayRealOrderEndpointOperation::PlaceOrder,
    )?;
    Ok(PlaceEndpointDurableCheckpointApproved { _private: () })
}

fn create_cancel_checkpoint_marker_after_sqlite_transition(
    _gate: &EndpointGateApproved,
    proof: &GatewayRealOrderEndpointSqliteTransitionCommitProof,
) -> Result<
    CancelEndpointDurableCheckpointApproved,
    GatewayRealOrderEndpointCheckpointMarkerCreationError,
> {
    validate_checkpoint_marker_creation_proof(
        proof,
        OrderPathEvent::RequestCancel,
        GatewayRealOrderEndpointOperation::CancelOrder,
    )?;
    Ok(CancelEndpointDurableCheckpointApproved { _private: () })
}

fn validate_checkpoint_marker_creation_proof(
    proof: &GatewayRealOrderEndpointSqliteTransitionCommitProof,
    expected_event: OrderPathEvent,
    expected_operation: GatewayRealOrderEndpointOperation,
) -> Result<(), GatewayRealOrderEndpointCheckpointMarkerCreationError> {
    if proof.diagnostic_or_report_source {
        return Err(GatewayRealOrderEndpointCheckpointMarkerCreationError::DiagnosticOrReportLayer);
    }
    if !proof.marker_single_use {
        return Err(GatewayRealOrderEndpointCheckpointMarkerCreationError::MarkerAlreadyUsed);
    }
    if !proof.durable_commit_observed {
        return Err(
            GatewayRealOrderEndpointCheckpointMarkerCreationError::DurableCommitNotObserved,
        );
    }
    if proof.event != expected_event {
        return Err(GatewayRealOrderEndpointCheckpointMarkerCreationError::WrongTransitionEvent);
    }
    validate_request_snapshot_fingerprint(&proof.request_snapshot_fingerprint, expected_operation)?;
    Ok(())
}

fn validate_request_snapshot_fingerprint(
    fingerprint: &GatewayRealOrderEndpointRequestSnapshotFingerprint,
    expected_operation: GatewayRealOrderEndpointOperation,
) -> Result<(), GatewayRealOrderEndpointCheckpointMarkerCreationError> {
    if fingerprint.raw_values_exported {
        return Err(
            GatewayRealOrderEndpointCheckpointMarkerCreationError::RawRequestIdentityExported,
        );
    }
    if fingerprint.operation != expected_operation || !fingerprint.matches_approved_request_parts {
        return Err(
            GatewayRealOrderEndpointCheckpointMarkerCreationError::RequestSnapshotFingerprintMismatch,
        );
    }
    if !(fingerprint.request_id_hash_present
        && fingerprint.client_order_id_hash_present
        && fingerprint.account_hash_present
        && fingerprint.instrument_hash_present
        && fingerprint.fingerprint_sha256_len == 64)
    {
        return Err(
            GatewayRealOrderEndpointCheckpointMarkerCreationError::RequestSnapshotFingerprintMissing,
        );
    }
    Ok(())
}

fn checkpoint_marker_creation_constructor_count() -> usize {
    let _place: fn(
        &EndpointGateApproved,
        &GatewayRealOrderEndpointSqliteTransitionCommitProof,
    ) -> Result<
        PlaceEndpointDurableCheckpointApproved,
        GatewayRealOrderEndpointCheckpointMarkerCreationError,
    > = create_place_checkpoint_marker_after_sqlite_transition;
    let _cancel: fn(
        &EndpointGateApproved,
        &GatewayRealOrderEndpointSqliteTransitionCommitProof,
    ) -> Result<
        CancelEndpointDurableCheckpointApproved,
        GatewayRealOrderEndpointCheckpointMarkerCreationError,
    > = create_cancel_checkpoint_marker_after_sqlite_transition;
    2
}

pub fn place_order_api_shape(
    _gate: &EndpointGateApproved,
    _spec: &broker_finam::FinamPlaceOrderRequestSpec,
) -> GatewayRealOrderEndpointRedactedRouteDiagnostic {
    redacted_route_diagnostic(place_order_route_shape())
}

pub fn cancel_order_api_shape(
    _gate: &EndpointGateApproved,
    _spec: &broker_finam::FinamCancelOrderRequestSpec,
) -> GatewayRealOrderEndpointRedactedRouteDiagnostic {
    redacted_route_diagnostic(cancel_order_route_shape())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request_fingerprint(
        operation: GatewayRealOrderEndpointOperation,
    ) -> GatewayRealOrderEndpointRequestSnapshotFingerprint {
        GatewayRealOrderEndpointRequestSnapshotFingerprint {
            operation,
            request_id_hash_present: true,
            client_order_id_hash_present: true,
            account_hash_present: true,
            instrument_hash_present: true,
            fingerprint_sha256_len: 64,
            raw_values_exported: false,
            matches_approved_request_parts: true,
        }
    }

    #[test]
    fn api_shape_is_design_only_and_requires_gate_marker() {
        let shape = api_shape();

        assert_eq!(
            shape.mode,
            GatewayRealOrderEndpointBoundaryMode::DesignOnlyNoHttpSend
        );
        assert_eq!(
            shape.approved_module_path,
            "crates/finam-gateway/src/real_order_endpoint.rs"
        );
        assert!(shape.route_rendering_requires_gate_marker);
        assert!(shape.http_send_requires_gate_marker);
        assert_eq!(
            shape.runtime_ack_id_policy,
            RuntimeCommandAckIdPolicy::RedactedRuntimeAckOnly
        );
        assert!(!shape.api_shape_contains_route_templates);
        assert!(
            shape
                .approved_request_parts_design
                .approved_request_parts_type_internal
        );
        assert!(
            shape
                .approved_request_parts_design
                .rendered_path_type_internal
        );
        assert!(!shape.approved_request_parts_design.rendered_path_exported);
        assert!(!shape.approved_request_parts_design.raw_body_exported);
        assert!(
            !shape
                .approved_request_parts_design
                .diagnostic_can_construct_request_parts
        );
        assert!(
            shape
                .approved_request_parts_design
                .constructors_require_endpoint_gate
        );
        assert!(
            shape
                .approved_request_parts_design
                .constructors_require_approved_request_spec
        );
        assert!(
            shape
                .approved_request_parts_design
                .constructors_require_account_instrument_allowlist
        );
        assert!(
            shape
                .approved_request_parts_design
                .constructors_require_operator_arm
        );
        assert!(
            shape
                .approved_request_parts_design
                .constructors_require_durable_state_checkpoint
        );
        assert!(
            shape
                .approved_request_parts_design
                .constructors_require_operation_specific_checkpoint
        );
        assert_eq!(shape.approved_request_parts_design.constructor_count, 2);
        assert!(shape.consumer_design.consumer_internal_only);
        assert!(shape.consumer_design.consumer_requires_endpoint_gate);
        assert!(
            shape
                .consumer_design
                .consumer_accepts_approved_request_parts_only
        );
        assert!(!shape.consumer_design.consumer_accepts_diagnostics);
        assert!(!shape.consumer_design.consumer_network_enabled);
        assert!(!shape.consumer_design.rendered_path_exported);
        assert!(!shape.consumer_design.raw_body_exported);
        assert!(shape.consumer_design.runtime_ack_redacted_only);
        assert_eq!(shape.consumer_design.consumer_count, 1);
        assert!(shape.future_send_result_design.design_only);
        assert_eq!(shape.future_send_result_design.outcome_count, 8);
        assert!(
            shape
                .future_send_result_design
                .future_send_requires_endpoint_gate
        );
        assert!(
            shape
                .future_send_result_design
                .future_send_accepts_approved_request_parts_only
        );
        assert!(
            !shape
                .future_send_result_design
                .future_send_accepts_diagnostics
        );
        assert!(
            shape
                .future_send_result_design
                .future_send_consumes_request_parts
        );
        assert!(!shape.future_send_result_design.future_send_network_enabled);
        assert!(
            shape
                .future_send_result_design
                .operation_specific_durable_checkpoint_required
        );
        assert_eq!(
            shape.future_send_result_design.place_checkpoint_label,
            GatewayRealOrderEndpointDurableCheckpointLabel::PlaceBeginSubmitPersistedBeforeEndpoint
        );
        assert_eq!(
            shape.future_send_result_design.cancel_checkpoint_label,
            GatewayRealOrderEndpointDurableCheckpointLabel::CancelRequestCancelPersistedBeforeEndpoint
        );
        assert!(
            !shape
                .future_send_result_design
                .retry_after_timeout_unknown_allowed
        );
        assert!(
            !shape
                .future_send_result_design
                .request_parts_reuse_after_outcome_allowed
        );
        assert!(
            !shape
                .future_send_result_design
                .result_diagnostic_can_bypass_state_machine
        );
        assert!(
            shape
                .future_send_result_design
                .state_machine_transition_required
        );
        assert!(!shape.future_send_result_design.rendered_path_exported);
        assert!(!shape.future_send_result_design.raw_body_exported);
        assert!(shape.future_send_result_design.runtime_ack_redacted_only);
        assert_eq!(shape.future_send_result_design.classifier_count, 1);
        assert!(shape.transport_category_policy_design.matrix_serializable);
        assert_eq!(shape.transport_category_policy_design.category_count, 5);
        assert_eq!(
            shape
                .transport_category_policy_design
                .timeout_category_count,
            1
        );
        assert_eq!(
            shape
                .transport_category_policy_design
                .non_timeout_category_count,
            4
        );
        assert!(
            shape
                .transport_category_policy_design
                .timeout_separated_from_non_timeout_transport
        );
        assert!(
            shape
                .transport_category_policy_design
                .non_timeout_transport_does_not_use_timeout_ack_reason
        );
        assert!(
            shape
                .transport_category_policy_design
                .non_timeout_transport_does_not_enter_timeout_unknown_state
        );
        assert!(
            shape
                .transport_category_policy_design
                .timeout_uses_unknown_pending_semantics
        );
        assert!(
            !shape
                .transport_category_policy_design
                .diagnostic_can_bypass_state_machine
        );
        assert!(shape.outcome_state_policy_design.matrix_serializable);
        assert_eq!(shape.outcome_state_policy_design.outcome_entry_count, 8);
        assert_eq!(
            shape
                .outcome_state_policy_design
                .accepted_broker_id_policy_entry_count,
            4
        );
        assert!(
            shape
                .outcome_state_policy_design
                .ack_reason_mapping_redacted
        );
        assert!(
            shape
                .outcome_state_policy_design
                .operator_disarm_backoff_manual_matrix_present
        );
        assert!(
            shape
                .outcome_state_policy_design
                .accepted_broker_id_policy_inherited
        );
        assert!(
            shape
                .outcome_state_policy_design
                .timeout_no_blind_retry_invariant
        );
        assert!(
            !shape
                .outcome_state_policy_design
                .outcome_diagnostic_can_bypass_state_machine
        );
        assert!(
            shape
                .outcome_state_policy_design
                .state_machine_transition_required
        );
        assert!(
            shape
                .accepted_result_classifier_design
                .classifier_internal_only
        );
        assert!(
            shape
                .accepted_result_classifier_design
                .classifier_requires_endpoint_gate
        );
        assert!(
            shape
                .accepted_result_classifier_design
                .classifier_accepts_approved_request_parts_only
        );
        assert!(
            !shape
                .accepted_result_classifier_design
                .classifier_accepts_diagnostics
        );
        assert!(
            shape
                .accepted_result_classifier_design
                .classifier_consumes_request_parts
        );
        assert_eq!(
            shape.accepted_result_classifier_design.accepted_kind_count,
            4
        );
        assert_eq!(
            shape
                .accepted_result_classifier_design
                .accepted_policy_entry_count,
            4
        );
        assert!(
            shape
                .accepted_result_classifier_design
                .accepted_broker_id_policy_wired
        );
        assert!(
            !shape
                .accepted_result_classifier_design
                .raw_broker_order_id_exported
        );
        assert!(
            !shape
                .accepted_result_classifier_design
                .unconditional_submitted_allowed
        );
        assert!(
            shape
                .accepted_result_classifier_design
                .state_machine_transition_required
        );
        assert_eq!(shape.accepted_result_classifier_design.classifier_count, 1);
        assert_eq!(shape.cancel_accepted_id_policy_design.policy_entry_count, 3);
        assert!(
            shape
                .cancel_accepted_id_policy_design
                .response_body_optional_documented
        );
        assert!(shape.cancel_accepted_id_policy_design.matching_id_ok);
        assert!(
            shape
                .cancel_accepted_id_policy_design
                .missing_id_requires_reconciliation
        );
        assert!(
            shape
                .cancel_accepted_id_policy_design
                .mismatched_id_manual_conflict
        );
        assert!(
            !shape
                .cancel_accepted_id_policy_design
                .raw_broker_order_id_exported
        );
        assert!(
            shape
                .cancel_accepted_id_policy_design
                .no_blind_retry_required
        );
        assert!(
            shape
                .captured_envelope_design
                .envelope_diagnostic_redacted_only
        );
        assert!(
            shape
                .captured_envelope_design
                .envelope_requires_endpoint_gate
        );
        assert!(
            shape
                .captured_envelope_design
                .envelope_accepts_approved_request_parts_only
        );
        assert!(
            !shape
                .captured_envelope_design
                .envelope_accepts_raw_path_body_or_error
        );
        assert!(!shape.captured_envelope_design.raw_path_exported);
        assert!(!shape.captured_envelope_design.raw_body_exported);
        assert!(!shape.captured_envelope_design.raw_error_exported);
        assert!(shape.captured_envelope_design.status_len_hash_presence_only);
        assert!(shape.captured_envelope_design.transport_category_mapped);
        assert_eq!(
            shape
                .captured_envelope_design
                .transport_category_mapping_entry_count,
            5
        );
        assert!(!shape.captured_envelope_design.diagnostic_can_feed_transport);
        assert!(shape.captured_envelope_design.runtime_ack_redacted_only);
        assert_eq!(shape.captured_envelope_design.diagnostic_count, 1);
        assert!(shape.endpoint_attempt_journal_design.journal_internal_only);
        assert_eq!(
            shape
                .endpoint_attempt_journal_design
                .endpoint_attempt_id_hash_len,
            64
        );
        assert!(
            !shape
                .endpoint_attempt_journal_design
                .attempt_id_raw_exported
        );
        assert!(
            shape
                .endpoint_attempt_journal_design
                .binds_approved_request_parts
        );
        assert!(
            shape
                .endpoint_attempt_journal_design
                .binds_request_snapshot_fingerprint
        );
        assert!(
            shape
                .endpoint_attempt_journal_design
                .binds_checkpoint_marker
        );
        assert!(
            shape
                .endpoint_attempt_journal_design
                .binds_captured_envelope
        );
        assert!(
            shape
                .endpoint_attempt_journal_design
                .binds_outcome_classifier
        );
        assert!(!shape.endpoint_attempt_journal_design.raw_path_exported);
        assert!(!shape.endpoint_attempt_journal_design.raw_body_exported);
        assert!(!shape.endpoint_attempt_journal_design.raw_error_exported);
        assert!(
            shape
                .endpoint_attempt_journal_design
                .diagnostic_redacted_only
        );
        assert!(
            !shape
                .endpoint_attempt_journal_design
                .diagnostic_can_feed_transport
        );
        assert!(
            !shape
                .endpoint_attempt_journal_design
                .diagnostic_can_bypass_state_machine
        );
        assert_eq!(shape.endpoint_attempt_journal_design.constructor_count, 2);
        assert_eq!(shape.endpoint_attempt_journal_design.diagnostic_count, 1);
        assert!(
            shape
                .durable_attempt_journal_contract_design
                .durable_journal_schema_design_only
        );
        assert!(
            shape
                .durable_attempt_journal_contract_design
                .journal_record_internal_only
        );
        assert!(
            shape
                .durable_attempt_journal_contract_design
                .append_requires_endpoint_gate
        );
        assert!(
            shape
                .durable_attempt_journal_contract_design
                .append_requires_approved_request_parts
        );
        assert!(
            shape
                .durable_attempt_journal_contract_design
                .append_requires_operation_specific_checkpoint_marker
        );
        assert_eq!(
            shape
                .durable_attempt_journal_contract_design
                .endpoint_attempt_id_hash_len,
            64
        );
        assert!(
            shape
                .durable_attempt_journal_contract_design
                .binds_request_fingerprint
        );
        assert!(
            shape
                .durable_attempt_journal_contract_design
                .binds_checkpoint_proof_fingerprint
        );
        assert!(
            shape
                .durable_attempt_journal_contract_design
                .binds_captured_envelope_fingerprint
        );
        assert!(
            shape
                .durable_attempt_journal_contract_design
                .binds_outcome_fingerprint
        );
        assert!(
            shape
                .durable_attempt_journal_contract_design
                .binds_state_transition_result_fingerprint
        );
        assert!(
            shape
                .durable_attempt_journal_contract_design
                .binds_ack_diagnostic_fingerprint
        );
        assert!(
            shape
                .durable_attempt_journal_contract_design
                .append_committed_after_state_transition
        );
        assert!(
            !shape
                .durable_attempt_journal_contract_design
                .raw_endpoint_attempt_id_exported
        );
        assert!(
            !shape
                .durable_attempt_journal_contract_design
                .raw_request_values_exported
        );
        assert!(
            !shape
                .durable_attempt_journal_contract_design
                .raw_broker_order_id_exported
        );
        assert!(
            !shape
                .durable_attempt_journal_contract_design
                .raw_path_exported
        );
        assert!(
            !shape
                .durable_attempt_journal_contract_design
                .raw_body_exported
        );
        assert!(
            !shape
                .durable_attempt_journal_contract_design
                .raw_error_exported
        );
        assert!(
            shape
                .durable_attempt_journal_contract_design
                .diagnostic_redacted_only
        );
        assert!(
            !shape
                .durable_attempt_journal_contract_design
                .diagnostic_can_feed_transport
        );
        assert!(
            !shape
                .durable_attempt_journal_contract_design
                .diagnostic_can_bypass_state_machine
        );
        assert!(
            shape
                .durable_attempt_journal_contract_design
                .exact_once_attempt_id_unique_required
        );
        assert!(
            shape
                .durable_attempt_journal_contract_design
                .replay_requires_same_fingerprint_set
        );
        assert_eq!(
            shape
                .durable_attempt_journal_contract_design
                .append_constructor_count,
            2
        );
        assert_eq!(
            shape
                .durable_attempt_journal_contract_design
                .diagnostic_count,
            1
        );
        assert!(shape.http_status_outcome_matrix_design.matrix_serializable);
        assert_eq!(
            shape.http_status_outcome_matrix_design.place_entry_count,
            15
        );
        assert_eq!(
            shape.http_status_outcome_matrix_design.cancel_entry_count,
            15
        );
        assert_eq!(
            shape.http_status_outcome_matrix_design.total_entry_count,
            30
        );
        assert!(
            shape
                .http_status_outcome_matrix_design
                .covers_2xx_accepted_body_variants
        );
        assert!(
            shape
                .http_status_outcome_matrix_design
                .covers_400_422_broker_reject
        );
        assert!(
            shape
                .http_status_outcome_matrix_design
                .covers_401_403_unauthorized
        );
        assert!(
            shape
                .http_status_outcome_matrix_design
                .covers_408_504_timeout
        );
        assert!(
            shape
                .http_status_outcome_matrix_design
                .covers_429_rate_limit
        );
        assert!(
            shape
                .http_status_outcome_matrix_design
                .covers_500_502_503_maintenance
        );
        assert!(
            shape
                .http_status_outcome_matrix_design
                .covers_malformed_body_decode_error
        );
        assert!(
            shape
                .http_status_outcome_matrix_design
                .covers_transport_category_failures
        );
        assert!(
            shape
                .http_status_outcome_matrix_design
                .place_cancel_specific_mapping
        );
        assert!(!shape.http_status_outcome_matrix_design.raw_path_exported);
        assert!(!shape.http_status_outcome_matrix_design.raw_body_exported);
        assert!(!shape.http_status_outcome_matrix_design.raw_error_exported);
        assert!(
            shape
                .http_status_outcome_matrix_design
                .state_machine_transition_required
        );
        assert!(
            shape
                .finam_status_semantics_design
                .official_rest_docs_checked
        );
        assert_eq!(
            shape
                .finam_status_semantics_design
                .documented_place_status_count,
            4
        );
        assert_eq!(
            shape
                .finam_status_semantics_design
                .documented_cancel_status_count,
            5
        );
        assert_eq!(
            shape.finam_status_semantics_design.place_status_entry_count,
            7
        );
        assert_eq!(
            shape
                .finam_status_semantics_design
                .cancel_status_entry_count,
            10
        );
        assert_eq!(
            shape.finam_status_semantics_design.total_status_entry_count,
            17
        );
        assert!(
            shape
                .finam_status_semantics_design
                .documented_success_status_only_200
        );
        assert!(
            shape
                .finam_status_semantics_design
                .undocumented_success_201_202_204_require_evidence_or_waiver
        );
        assert!(
            shape
                .finam_status_semantics_design
                .place_success_body_required_for_immediate_submitted
        );
        assert!(
            shape
                .finam_status_semantics_design
                .place_empty_body_requires_reconciliation
        );
        assert!(
            shape
                .finam_status_semantics_design
                .cancel_success_body_optional
        );
        assert!(
            shape
                .finam_status_semantics_design
                .cancel_missing_id_requires_reconciliation
        );
        assert!(
            shape
                .finam_status_semantics_design
                .cancel_404_documented_and_requires_reconciliation
        );
        assert!(
            !shape
                .finam_status_semantics_design
                .cancel_409_410_documented_by_finam_rest_docs
        );
        assert!(
            shape
                .finam_status_semantics_design
                .cancel_409_410_policy_or_waiver_required
        );
        assert!(
            shape
                .finam_status_semantics_design
                .defensive_422_502_not_documented_as_finam_order_status
        );
        assert!(
            !shape
                .finam_status_semantics_design
                .status_semantics_can_bypass_state_machine
        );
        assert!(!shape.finam_status_semantics_design.raw_path_exported);
        assert!(!shape.finam_status_semantics_design.raw_body_exported);
        assert!(!shape.finam_status_semantics_design.raw_error_exported);
        assert!(
            !shape
                .finam_status_semantics_design
                .raw_broker_order_id_exported
        );
        assert!(
            shape
                .implementation_gate_evidence_closure_plan_design
                .plan_design_only
        );
        assert_eq!(
            shape
                .implementation_gate_evidence_closure_plan_design
                .slot_count,
            5
        );
        assert!(
            shape
                .implementation_gate_evidence_closure_plan_design
                .release_profile_slot_present
        );
        assert!(
            shape
                .implementation_gate_evidence_closure_plan_design
                .positive_get_order_slot_present
        );
        assert!(
            shape
                .implementation_gate_evidence_closure_plan_design
                .route_template_recheck_slot_present
        );
        assert!(
            shape
                .implementation_gate_evidence_closure_plan_design
                .undocumented_2xx_policy_slot_present
        );
        assert!(
            shape
                .implementation_gate_evidence_closure_plan_design
                .cancel_409_410_policy_slot_present
        );
        assert!(
            shape
                .implementation_gate_evidence_closure_plan_design
                .all_slots_pending_before_implementation_gate
        );
        assert!(
            shape
                .implementation_gate_evidence_closure_plan_design
                .all_slots_require_reviewer_acceptance
        );
        assert!(
            !shape
                .implementation_gate_evidence_closure_plan_design
                .order_endpoint_calls_allowed_for_closure
        );
        assert!(
            shape
                .implementation_gate_evidence_closure_plan_design
                .closure_artifacts_redacted_only
        );
        assert!(
            shape
                .implementation_gate_evidence_closure_plan_design
                .source_archive_binding_required
        );
        assert!(
            shape
                .durable_attempt_journal_sqlite_schema_design
                .schema_design_only
        );
        assert_eq!(
            shape
                .durable_attempt_journal_sqlite_schema_design
                .table_name,
            "order_endpoint_attempts"
        );
        assert_eq!(
            shape
                .durable_attempt_journal_sqlite_schema_design
                .schema_version,
            1
        );
        assert_eq!(
            shape
                .durable_attempt_journal_sqlite_schema_design
                .column_count,
            19
        );
        assert_eq!(
            shape
                .durable_attempt_journal_sqlite_schema_design
                .unique_index_count,
            1
        );
        assert_eq!(
            shape
                .durable_attempt_journal_sqlite_schema_design
                .replay_policy_entry_count,
            2
        );
        assert!(
            shape
                .durable_attempt_journal_sqlite_schema_design
                .endpoint_attempt_id_unique
        );
        assert!(
            shape
                .durable_attempt_journal_sqlite_schema_design
                .request_id_hash_indexed
        );
        assert!(
            shape
                .durable_attempt_journal_sqlite_schema_design
                .client_order_id_hash_indexed
        );
        assert!(
            shape
                .durable_attempt_journal_sqlite_schema_design
                .broker_order_id_hash_optional
        );
        assert!(
            shape
                .durable_attempt_journal_sqlite_schema_design
                .order_path_record_reference_required
        );
        assert!(
            shape
                .durable_attempt_journal_sqlite_schema_design
                .append_only
        );
        assert!(
            shape
                .durable_attempt_journal_sqlite_schema_design
                .begin_immediate_required
        );
        assert!(
            shape
                .durable_attempt_journal_sqlite_schema_design
                .wal_required
        );
        assert!(
            shape
                .durable_attempt_journal_sqlite_schema_design
                .synchronous_full_required
        );
        assert!(
            shape
                .durable_attempt_journal_sqlite_schema_design
                .writer_lock_required
        );
        assert!(
            shape
                .durable_attempt_journal_sqlite_schema_design
                .schema_version_guard_required
        );
        assert!(
            shape
                .durable_attempt_journal_sqlite_schema_design
                .idempotent_replay_requires_same_fingerprint_set
        );
        assert!(
            shape
                .durable_attempt_journal_sqlite_schema_design
                .conflict_replay_rejects_and_disarms
        );
        assert!(
            !shape
                .durable_attempt_journal_sqlite_schema_design
                .raw_endpoint_attempt_id_exported
        );
        assert!(
            !shape
                .durable_attempt_journal_sqlite_schema_design
                .raw_request_values_exported
        );
        assert!(
            !shape
                .durable_attempt_journal_sqlite_schema_design
                .raw_broker_order_id_exported
        );
        assert!(
            !shape
                .durable_attempt_journal_sqlite_schema_design
                .raw_path_exported
        );
        assert!(
            !shape
                .durable_attempt_journal_sqlite_schema_design
                .raw_body_exported
        );
        assert!(
            !shape
                .durable_attempt_journal_sqlite_schema_design
                .raw_error_exported
        );
        assert!(
            shape
                .durable_journal_migration_runbook_design
                .migration_runbook_design_only
        );
        assert_eq!(shape.durable_journal_migration_runbook_design.step_count, 9);
        assert!(
            shape
                .durable_journal_migration_runbook_design
                .backup_required_before_migration
        );
        assert!(
            shape
                .durable_journal_migration_runbook_design
                .begin_immediate_required_for_schema_change
        );
        assert!(shape.durable_journal_migration_runbook_design.wal_required);
        assert!(
            shape
                .durable_journal_migration_runbook_design
                .synchronous_full_required
        );
        assert!(
            shape
                .durable_journal_migration_runbook_design
                .single_writer_lock_required
        );
        assert!(
            shape
                .durable_journal_migration_runbook_design
                .schema_version_guard_required
        );
        assert!(
            shape
                .durable_journal_migration_runbook_design
                .sqlite_integrity_check_required
        );
        assert!(
            shape
                .durable_journal_migration_runbook_design
                .corruption_open_failure_disarms
        );
        assert!(
            shape
                .durable_journal_migration_runbook_design
                .stale_or_unknown_lock_disarms
        );
        assert!(
            !shape
                .durable_journal_migration_runbook_design
                .auto_repair_allowed
        );
        assert!(
            !shape
                .durable_journal_migration_runbook_design
                .auto_stale_lock_delete_allowed
        );
        assert!(
            shape
                .durable_journal_migration_runbook_design
                .operator_runbook_required
        );
        assert!(
            shape
                .durable_journal_migration_runbook_design
                .redacted_operator_diagnostics_only
        );
        assert!(
            !shape
                .durable_journal_migration_runbook_design
                .raw_sqlite_path_exported
        );
        assert!(
            !shape
                .durable_journal_migration_runbook_design
                .raw_request_values_exported
        );
        assert!(
            !shape
                .durable_journal_migration_runbook_design
                .raw_broker_payload_exported
        );
        assert!(
            shape
                .canonical_replay_fingerprint_design
                .fingerprint_spec_design_only
        );
        assert_eq!(shape.canonical_replay_fingerprint_design.field_count, 14);
        assert_eq!(
            shape.canonical_replay_fingerprint_design.encoding,
            GatewayRealOrderEndpointCanonicalReplayEncoding::Utf8JsonObjectSortedKeysNoWhitespace
        );
        assert!(
            shape
                .canonical_replay_fingerprint_design
                .schema_version_included
        );
        assert!(shape.canonical_replay_fingerprint_design.operation_included);
        assert!(
            shape
                .canonical_replay_fingerprint_design
                .endpoint_attempt_id_included
        );
        assert!(
            shape
                .canonical_replay_fingerprint_design
                .request_client_account_instrument_hashes_included
        );
        assert!(
            shape
                .canonical_replay_fingerprint_design
                .checkpoint_and_envelope_hashes_included
        );
        assert!(
            shape
                .canonical_replay_fingerprint_design
                .outcome_state_ack_hashes_included
        );
        assert!(
            shape
                .canonical_replay_fingerprint_design
                .stable_field_order_required
        );
        assert!(
            shape
                .canonical_replay_fingerprint_design
                .sorted_keys_required
        );
        assert!(
            shape
                .canonical_replay_fingerprint_design
                .whitespace_forbidden
        );
        assert_eq!(shape.canonical_replay_fingerprint_design.sha256_len, 64);
        assert!(
            !shape
                .canonical_replay_fingerprint_design
                .raw_values_exported
        );
        assert!(
            shape
                .canonical_replay_fingerprint_design
                .refactor_changes_require_schema_bump
        );
        assert!(
            shape
                .endpoint_attempt_id_lifecycle_design
                .lifecycle_design_only
        );
        assert_eq!(shape.endpoint_attempt_id_lifecycle_design.phase_count, 5);
        assert!(
            shape
                .endpoint_attempt_id_lifecycle_design
                .generated_after_approved_request_parts
        );
        assert!(
            shape
                .endpoint_attempt_id_lifecycle_design
                .generated_before_future_endpoint_send
        );
        assert!(
            shape
                .endpoint_attempt_id_lifecycle_design
                .bound_to_request_and_client_id_hashes
        );
        assert!(
            shape
                .endpoint_attempt_id_lifecycle_design
                .bound_to_operation
        );
        assert!(
            shape
                .endpoint_attempt_id_lifecycle_design
                .persisted_before_outcome_export
        );
        assert!(
            shape
                .endpoint_attempt_id_lifecycle_design
                .same_attempt_id_replay_requires_same_fingerprint_set
        );
        assert!(
            !shape
                .endpoint_attempt_id_lifecycle_design
                .reuse_after_timeout_manual_or_terminal_allowed
        );
        assert!(
            shape
                .endpoint_attempt_id_lifecycle_design
                .new_endpoint_attempt_requires_new_id
        );
        assert!(
            !shape
                .endpoint_attempt_id_lifecycle_design
                .raw_endpoint_attempt_id_exported
        );
        assert!(
            shape
                .implementation_gate_readiness_design
                .checklist_design_only
        );
        assert_eq!(
            shape
                .implementation_gate_readiness_design
                .checklist_entry_count,
            12
        );
        assert_eq!(
            shape
                .implementation_gate_readiness_design
                .implemented_tested_count,
            7
        );
        assert_eq!(
            shape
                .implementation_gate_readiness_design
                .pending_evidence_or_waiver_count,
            3
        );
        assert!(
            shape
                .implementation_gate_readiness_design
                .release_profile_evidence_or_waiver_pending
        );
        assert!(
            shape
                .implementation_gate_readiness_design
                .positive_get_order_evidence_or_waiver_pending
        );
        assert!(
            shape
                .implementation_gate_readiness_design
                .route_template_recheck_pending
        );
        assert!(
            shape
                .implementation_gate_readiness_design
                .canonical_replay_golden_vectors_present
        );
        assert!(
            shape
                .implementation_gate_readiness_design
                .operator_replay_runbook_present
        );
        assert!(
            !shape
                .implementation_gate_readiness_design
                .endpoint_calls_allowed_for_readiness
        );
        assert!(
            !shape
                .implementation_gate_readiness_design
                .raw_values_exported
        );
        assert!(
            shape
                .canonical_replay_golden_vector_design
                .golden_vectors_design_only
        );
        assert_eq!(shape.canonical_replay_golden_vector_design.vector_count, 1);
        assert!(
            shape
                .canonical_replay_golden_vector_design
                .canonical_json_no_whitespace
        );
        assert_eq!(
            shape
                .canonical_replay_golden_vector_design
                .expected_sha256_len,
            64
        );
        assert!(
            shape
                .canonical_replay_golden_vector_design
                .all_fields_hash_or_safe_label
        );
        assert!(
            !shape
                .canonical_replay_golden_vector_design
                .raw_values_exported
        );
        assert!(
            shape
                .canonical_replay_golden_vector_design
                .refactor_changes_require_vector_update_and_schema_bump
        );
        assert!(
            shape
                .operator_replay_runbook_design
                .operator_runbook_design_only
        );
        assert_eq!(shape.operator_replay_runbook_design.case_count, 5);
        assert!(
            shape
                .operator_replay_runbook_design
                .idempotent_replay_case_present
        );
        assert!(
            shape
                .operator_replay_runbook_design
                .conflicting_replay_disarms
        );
        assert!(
            shape
                .operator_replay_runbook_design
                .timeout_requires_new_attempt_id
        );
        assert!(
            shape
                .operator_replay_runbook_design
                .manual_requires_new_attempt_id
        );
        assert!(
            shape
                .operator_replay_runbook_design
                .terminal_requires_new_attempt_id
        );
        assert!(
            shape
                .operator_replay_runbook_design
                .redacted_diagnostics_only
        );
        assert!(
            !shape
                .operator_replay_runbook_design
                .raw_endpoint_attempt_id_exported
        );
        assert!(
            shape
                .evidence_closure_package_design
                .closure_package_design_only
        );
        assert_eq!(shape.evidence_closure_package_design.slot_count, 5);
        assert!(
            shape
                .evidence_closure_package_design
                .release_profile_slot_present
        );
        assert!(
            shape
                .evidence_closure_package_design
                .positive_get_order_slot_present
        );
        assert!(
            shape
                .evidence_closure_package_design
                .route_template_recheck_slot_present
        );
        assert!(
            shape
                .evidence_closure_package_design
                .undocumented_2xx_slot_present
        );
        assert!(
            shape
                .evidence_closure_package_design
                .cancel_409_410_slot_present
        );
        assert!(
            shape
                .evidence_closure_package_design
                .all_slots_require_evidence_or_waiver
        );
        assert!(
            shape
                .evidence_closure_package_design
                .all_slots_require_reviewer_acceptance
        );
        assert!(
            shape
                .evidence_closure_package_design
                .source_archive_binding_required
        );
        assert!(
            !shape
                .evidence_closure_package_design
                .order_endpoint_calls_allowed_for_closure
        );
        assert!(!shape.evidence_closure_package_design.raw_values_exported);
        assert!(
            shape
                .route_template_recheck_plan_design
                .route_template_recheck_design_only
        );
        assert_eq!(shape.route_template_recheck_plan_design.route_count, 2);
        assert!(
            shape
                .route_template_recheck_plan_design
                .exact_two_route_allowlist_required
        );
        assert!(
            shape
                .route_template_recheck_plan_design
                .official_docs_or_waiver_required
        );
        assert!(
            shape
                .route_template_recheck_plan_design
                .reviewer_acceptance_required
        );
        assert!(
            shape
                .route_template_recheck_plan_design
                .recheck_before_implementation_gate
        );
        assert!(
            shape
                .route_template_recheck_plan_design
                .route_templates_exported_as_design_data_only
        );
        assert!(
            !shape
                .route_template_recheck_plan_design
                .rendered_routes_exported
        );
        assert!(
            !shape
                .route_template_recheck_plan_design
                .raw_account_or_order_id_exported
        );
        assert!(
            !shape
                .route_template_recheck_plan_design
                .order_endpoint_calls_allowed_for_recheck
        );
        assert!(
            shape
                .evidence_report_readiness_design
                .evidence_report_readiness_design_only
        );
        assert_eq!(
            shape
                .evidence_report_readiness_design
                .canonical_replay_golden_vector_sha256,
            "d467afd3b7d320c26966a1a400995e00664397ed47bb74320a418cfd2524abc6"
        );
        assert_eq!(
            shape
                .evidence_report_readiness_design
                .canonical_replay_vector_count,
            1
        );
        assert_eq!(
            shape
                .evidence_report_readiness_design
                .readiness_implemented_tested_count,
            7
        );
        assert_eq!(
            shape
                .evidence_report_readiness_design
                .readiness_pending_evidence_or_waiver_count,
            3
        );
        assert_eq!(
            shape
                .evidence_report_readiness_design
                .operator_replay_runbook_case_count,
            5
        );
        assert!(!shape.evidence_report_readiness_design.raw_values_exported);
        assert!(
            shape
                .durable_checkpoint_capability_design
                .place_capability_type_internal
        );
        assert!(
            shape
                .durable_checkpoint_capability_design
                .cancel_capability_type_internal
        );
        assert!(
            shape
                .durable_checkpoint_capability_design
                .capability_not_debug_or_serializable
        );
        assert!(
            shape
                .durable_checkpoint_capability_design
                .created_after_sqlite_transition_only
        );
        assert_eq!(
            shape
                .durable_checkpoint_capability_design
                .place_required_event,
            OrderPathEvent::BeginSubmit
        );
        assert_eq!(
            shape
                .durable_checkpoint_capability_design
                .place_required_label,
            GatewayRealOrderEndpointDurableCheckpointLabel::PlaceBeginSubmitPersistedBeforeEndpoint
        );
        assert_eq!(
            shape
                .durable_checkpoint_capability_design
                .cancel_required_event,
            OrderPathEvent::RequestCancel
        );
        assert_eq!(
            shape
                .durable_checkpoint_capability_design
                .cancel_required_label,
            GatewayRealOrderEndpointDurableCheckpointLabel::CancelRequestCancelPersistedBeforeEndpoint
        );
        assert!(
            shape
                .checkpoint_marker_creation_design
                .place_marker_creation_function_private
        );
        assert!(
            shape
                .checkpoint_marker_creation_design
                .cancel_marker_creation_function_private
        );
        assert!(
            shape
                .checkpoint_marker_creation_design
                .creation_requires_endpoint_gate
        );
        assert!(
            shape
                .checkpoint_marker_creation_design
                .creation_requires_sqlite_transition_commit_proof
        );
        assert!(
            shape
                .checkpoint_marker_creation_design
                .proof_bound_to_request_snapshot_fingerprint
        );
        assert!(
            shape
                .checkpoint_marker_creation_design
                .proof_fingerprint_includes_request_client_account_instrument_hashes
        );
        assert!(
            !shape
                .checkpoint_marker_creation_design
                .proof_raw_request_values_exported
        );
        assert!(
            shape
                .checkpoint_marker_creation_design
                .marker_single_use_required
        );
        assert!(
            !shape
                .checkpoint_marker_creation_design
                .checkpoint_reuse_across_intents_allowed
        );
        assert!(
            shape
                .checkpoint_marker_creation_design
                .creation_rejects_diagnostic_or_report_source
        );
        assert!(
            shape
                .checkpoint_marker_creation_design
                .creation_requires_durable_commit_observed
        );
        assert!(
            shape
                .checkpoint_marker_creation_design
                .creation_requires_transition_event_match
        );
        assert!(
            shape
                .checkpoint_marker_creation_design
                .creation_requires_fingerprint_operation_match
        );
        assert_eq!(
            shape.checkpoint_marker_creation_design.place_required_event,
            OrderPathEvent::BeginSubmit
        );
        assert_eq!(
            shape
                .checkpoint_marker_creation_design
                .cancel_required_event,
            OrderPathEvent::RequestCancel
        );
        assert!(
            shape
                .checkpoint_marker_creation_design
                .marker_not_debug_or_serializable
        );
        assert_eq!(shape.checkpoint_marker_creation_design.constructor_count, 2);
        assert!(
            !shape
                .scanner_transition_spec
                .real_post_delete_calls_allowed_now
        );
        assert_eq!(
            shape.scanner_transition_spec.current_mode,
            M3cOrderEndpointScannerTransitionMode::CurrentDenyAllOrderPostDelete
        );
        assert_eq!(
            shape.scanner_transition_spec.future_mode,
            M3cOrderEndpointScannerTransitionMode::FutureExactTwoRouteAllowlistAfterReview
        );
        assert_eq!(
            shape.scanner_transition_spec.allowed_route_template_count,
            2
        );
        assert_eq!(shape.scanner_transition_spec.negative_tests.len(), 6);
        let rendered = serde_json::to_string(&shape).expect("shape serializes");
        assert!(!rendered.contains("/v1/accounts/{account_id}/orders"));
        assert!(!rendered.contains("ApprovedOrderEndpointRequestParts"));
        assert!(!rendered.contains("RenderedOrderEndpointPath"));
    }

    #[test]
    fn route_shape_functions_require_endpoint_gate_marker_in_signature() {
        fn assert_place_signature(
            _f: fn(
                &EndpointGateApproved,
                &broker_finam::FinamPlaceOrderRequestSpec,
            ) -> GatewayRealOrderEndpointRedactedRouteDiagnostic,
        ) {
        }
        fn assert_cancel_signature(
            _f: fn(
                &EndpointGateApproved,
                &broker_finam::FinamCancelOrderRequestSpec,
            ) -> GatewayRealOrderEndpointRedactedRouteDiagnostic,
        ) {
        }

        assert_place_signature(place_order_api_shape);
        assert_cancel_signature(cancel_order_api_shape);
    }

    #[test]
    fn internal_route_shapes_are_separate_from_design_report_shape() {
        let place = place_order_route_shape();
        let cancel = cancel_order_route_shape();

        assert_eq!(place.method_name, "POST");
        assert_eq!(cancel.method_name, "DELETE");
        assert_eq!(place.route_template, "/v1/accounts/{account_id}/orders");
        assert_eq!(
            cancel.route_template,
            "/v1/accounts/{account_id}/orders/{order_id}"
        );
        assert!(place.gate_marker_required);
        assert!(cancel.gate_marker_required);
    }

    #[test]
    fn exported_route_diagnostics_are_redacted_and_not_transport_input() {
        let place = redacted_route_diagnostic(place_order_route_shape());
        let cancel = redacted_route_diagnostic(cancel_order_route_shape());

        assert!(place.route_template_redacted);
        assert!(cancel.route_template_redacted);
        assert!(!place.route_template_exported);
        assert!(!cancel.route_template_exported);

        let rendered = serde_json::to_string(&[place, cancel]).expect("diagnostics serialize");
        assert!(!rendered.contains("/v1/accounts/{account_id}/orders"));
        assert!(!rendered.contains("{order_id}"));
        assert!(rendered.contains("\"route_template_redacted\":true"));
        assert!(rendered.contains("\"route_template_exported\":false"));
    }

    #[test]
    fn approved_request_parts_constructors_require_all_safety_inputs() {
        fn assert_place_signature(
            _f: fn(
                &EndpointGateApproved,
                &broker_finam::FinamPlaceOrderRequestSpec,
                &OrderEndpointAccountInstrumentAllowlistApproved,
                &OrderEndpointOperatorArmApproved,
                &OrderEndpointDurableStateCheckpoint,
            ) -> Result<
                ApprovedOrderEndpointRequestParts,
                GatewayRealOrderEndpointApprovedPartsError,
            >,
        ) {
        }
        fn assert_cancel_signature(
            _f: fn(
                &EndpointGateApproved,
                &broker_finam::FinamCancelOrderRequestSpec,
                &OrderEndpointAccountInstrumentAllowlistApproved,
                &OrderEndpointOperatorArmApproved,
                &OrderEndpointDurableStateCheckpoint,
            ) -> Result<
                ApprovedOrderEndpointRequestParts,
                GatewayRealOrderEndpointApprovedPartsError,
            >,
        ) {
        }

        assert_place_signature(build_place_approved_request_parts);
        assert_cancel_signature(build_cancel_approved_request_parts);
        assert_eq!(approved_request_parts_constructor_count(), 2);
    }

    #[test]
    fn approved_request_parts_diagnostic_does_not_export_raw_path_or_body() {
        let place_parts = ApprovedOrderEndpointRequestParts {
            operation: GatewayRealOrderEndpointOperation::PlaceOrder,
            method_name: "POST",
            rendered_path: RenderedOrderEndpointPath(
                "/v1/accounts/ACC_TEST_0001/orders".to_string(),
            ),
            approved_request_spec: ApprovedOrderEndpointRequestSpec::Place(
                broker_finam::FinamPlaceOrderRequestSpec {
                    account_id: "ACC_TEST_0001".to_string(),
                    body: broker_finam::FinamPlaceOrderRequest {
                        symbol: "IMOEXF_TEST".to_string(),
                        quantity: broker_finam::DecimalValue {
                            value: "1".to_string(),
                        },
                        side: "BUY".to_string(),
                        order_type: "ORDER_TYPE_MARKET".to_string(),
                        time_in_force: Some("TIME_IN_FORCE_DAY".to_string()),
                        limit_price: None,
                        client_order_id: Some("CID_TEST_0001".to_string()),
                        comment: None,
                    },
                },
            ),
            account_instrument_allowlist_approved: true,
            operator_arm_approved: true,
            durable_state_checkpoint_present: true,
            durable_checkpoint_label:
                GatewayRealOrderEndpointDurableCheckpointLabel::PlaceBeginSubmitPersistedBeforeEndpoint,
        };
        let cancel_parts = ApprovedOrderEndpointRequestParts {
            operation: GatewayRealOrderEndpointOperation::CancelOrder,
            method_name: "DELETE",
            rendered_path: RenderedOrderEndpointPath(
                "/v1/accounts/ACC_TEST_0001/orders/ORDER_TEST_0001".to_string(),
            ),
            approved_request_spec: ApprovedOrderEndpointRequestSpec::Cancel(
                broker_finam::FinamCancelOrderRequestSpec {
                    account_id: "ACC_TEST_0001".to_string(),
                    order_id: "ORDER_TEST_0001".to_string(),
                },
            ),
            account_instrument_allowlist_approved: true,
            operator_arm_approved: true,
            durable_state_checkpoint_present: true,
            durable_checkpoint_label:
                GatewayRealOrderEndpointDurableCheckpointLabel::CancelRequestCancelPersistedBeforeEndpoint,
        };

        let place = approved_request_parts_redacted_diagnostic(&place_parts);
        let cancel = approved_request_parts_redacted_diagnostic(&cancel_parts);
        let rendered = serde_json::to_string(&[place, cancel]).expect("diagnostics serialize");

        assert!(!rendered.contains("/v1/accounts/ACC_TEST_0001"));
        assert!(!rendered.contains("ACC_TEST_0001"));
        assert!(!rendered.contains("ORDER_TEST_0001"));
        assert!(!rendered.contains("IMOEXF_TEST"));
        assert!(!rendered.contains("CID_TEST_0001"));
        assert!(rendered.contains("\"rendered_path_redacted\":true"));
        assert!(rendered.contains("\"rendered_path_exported\":false"));
        assert!(rendered.contains("\"raw_body_exported\":false"));
    }

    #[test]
    fn diagnostics_cannot_feed_request_parts_constructors() {
        let source = include_str!("real_order_endpoint.rs");
        let constructor_source = source
            .split("fn build_place_approved_request_parts")
            .nth(1)
            .expect("place constructor")
            .split("fn approved_request_parts_redacted_diagnostic")
            .next()
            .expect("constructor boundary");

        assert!(!constructor_source.contains("GatewayRealOrderEndpointRedactedRouteDiagnostic"));
        assert!(!constructor_source.contains("GatewayRealOrderEndpointApprovedPartsDiagnostic"));
        assert!(constructor_source.contains("EndpointGateApproved"));
        assert!(constructor_source.contains("FinamPlaceOrderRequestSpec"));
        assert!(constructor_source.contains("FinamCancelOrderRequestSpec"));
        assert!(constructor_source.contains("OrderEndpointAccountInstrumentAllowlistApproved"));
        assert!(constructor_source.contains("OrderEndpointOperatorArmApproved"));
        assert!(constructor_source.contains("OrderEndpointDurableStateCheckpoint"));
    }

    #[test]
    fn approved_request_parts_consumer_requires_gate_and_internal_parts() {
        fn assert_consumer_signature(
            _f: fn(
                &EndpointGateApproved,
                ApprovedOrderEndpointRequestParts,
            ) -> GatewayRealOrderEndpointConsumerDiagnostic,
        ) {
        }

        assert_consumer_signature(consume_approved_request_parts_for_future_endpoint);
        assert_eq!(approved_request_parts_consumer_count(), 1);
    }

    #[test]
    fn approved_request_parts_consumer_diagnostic_is_redacted() {
        let parts = ApprovedOrderEndpointRequestParts {
            operation: GatewayRealOrderEndpointOperation::PlaceOrder,
            method_name: "POST",
            rendered_path: RenderedOrderEndpointPath(
                "/v1/accounts/ACC_TEST_0001/orders".to_string(),
            ),
            approved_request_spec: ApprovedOrderEndpointRequestSpec::Place(
                broker_finam::FinamPlaceOrderRequestSpec {
                    account_id: "ACC_TEST_0001".to_string(),
                    body: broker_finam::FinamPlaceOrderRequest {
                        symbol: "IMOEXF_TEST".to_string(),
                        quantity: broker_finam::DecimalValue {
                            value: "1".to_string(),
                        },
                        side: "BUY".to_string(),
                        order_type: "ORDER_TYPE_MARKET".to_string(),
                        time_in_force: Some("TIME_IN_FORCE_DAY".to_string()),
                        limit_price: None,
                        client_order_id: Some("CID_TEST_0002".to_string()),
                        comment: None,
                    },
                },
            ),
            account_instrument_allowlist_approved: true,
            operator_arm_approved: true,
            durable_state_checkpoint_present: true,
            durable_checkpoint_label:
                GatewayRealOrderEndpointDurableCheckpointLabel::PlaceBeginSubmitPersistedBeforeEndpoint,
        };
        let diagnostic = approved_request_parts_consumer_redacted_diagnostic(parts);

        let rendered = serde_json::to_string(&diagnostic).expect("diagnostic serializes");
        assert!(!rendered.contains("/v1/accounts/ACC_TEST_0001"));
        assert!(!rendered.contains("ACC_TEST_0001"));
        assert!(!rendered.contains("IMOEXF_TEST"));
        assert!(!rendered.contains("CID_TEST_0002"));
        assert!(rendered.contains("\"accepted_approved_request_parts\":true"));
        assert!(rendered.contains("\"endpoint_gate_required\":true"));
        assert!(rendered.contains("\"network_enabled\":false"));
        assert!(rendered.contains("\"rendered_path_redacted\":true"));
        assert!(rendered.contains("\"rendered_path_exported\":false"));
        assert!(rendered.contains("\"raw_body_exported\":false"));
        assert!(rendered.contains("\"runtime_ack_redacted_only\":true"));
    }

    #[test]
    fn diagnostics_cannot_feed_consumer_boundary() {
        let source = include_str!("real_order_endpoint.rs");
        let consumer_source = source
            .split("fn consume_approved_request_parts_for_future_endpoint")
            .nth(1)
            .expect("consumer source")
            .split("fn approved_request_parts_constructor_count")
            .next()
            .expect("consumer boundary");

        assert!(consumer_source.contains("EndpointGateApproved"));
        assert!(consumer_source.contains("ApprovedOrderEndpointRequestParts"));
        assert!(!consumer_source.contains("GatewayRealOrderEndpointRedactedRouteDiagnostic"));
        assert!(!consumer_source.contains("GatewayRealOrderEndpointApprovedPartsDiagnostic"));
    }

    #[test]
    fn operation_specific_durable_checkpoint_labels_are_required() {
        let allowlist = OrderEndpointAccountInstrumentAllowlistApproved {
            account_allowlisted: true,
            instrument_allowlisted: true,
        };
        let operator_arm = OrderEndpointOperatorArmApproved {
            operator_arm_validated: true,
            one_shot_arm: true,
        };
        let place_checkpoint = OrderEndpointDurableStateCheckpoint {
            intent_recorded_before_endpoint: true,
            label:
                GatewayRealOrderEndpointDurableCheckpointLabel::PlaceBeginSubmitPersistedBeforeEndpoint,
        };
        let cancel_checkpoint = OrderEndpointDurableStateCheckpoint {
            intent_recorded_before_endpoint: true,
            label:
                GatewayRealOrderEndpointDurableCheckpointLabel::CancelRequestCancelPersistedBeforeEndpoint,
        };

        assert!(validate_request_part_inputs(
            GatewayRealOrderEndpointOperation::PlaceOrder,
            &allowlist,
            &operator_arm,
            &place_checkpoint,
        )
        .is_ok());
        assert_eq!(
            validate_request_part_inputs(
                GatewayRealOrderEndpointOperation::PlaceOrder,
                &allowlist,
                &operator_arm,
                &cancel_checkpoint,
            ),
            Err(GatewayRealOrderEndpointApprovedPartsError::DurableStateCheckpoint)
        );
        assert!(validate_request_part_inputs(
            GatewayRealOrderEndpointOperation::CancelOrder,
            &allowlist,
            &operator_arm,
            &cancel_checkpoint,
        )
        .is_ok());
        assert_eq!(
            validate_request_part_inputs(
                GatewayRealOrderEndpointOperation::CancelOrder,
                &allowlist,
                &operator_arm,
                &place_checkpoint,
            ),
            Err(GatewayRealOrderEndpointApprovedPartsError::DurableStateCheckpoint)
        );
    }

    #[test]
    fn future_send_outcome_shape_lists_expected_outcomes() {
        assert_eq!(future_send_outcome_count(), 8);
        assert_eq!(
            future_send_outcomes(),
            [
                GatewayRealOrderEndpointFutureSendOutcome::Accepted,
                GatewayRealOrderEndpointFutureSendOutcome::Rejected,
                GatewayRealOrderEndpointFutureSendOutcome::TimeoutUnknownPending,
                GatewayRealOrderEndpointFutureSendOutcome::RateLimited,
                GatewayRealOrderEndpointFutureSendOutcome::Maintenance,
                GatewayRealOrderEndpointFutureSendOutcome::Unauthorized,
                GatewayRealOrderEndpointFutureSendOutcome::DecodeError,
                GatewayRealOrderEndpointFutureSendOutcome::TransportError,
            ]
        );
    }

    #[test]
    fn future_send_result_classifier_requires_gate_and_consumes_parts() {
        fn assert_classifier_signature(
            _f: fn(
                &EndpointGateApproved,
                ApprovedOrderEndpointRequestParts,
                GatewayRealOrderEndpointFutureSendOutcome,
            ) -> GatewayRealOrderEndpointFutureSendDiagnostic,
        ) {
        }

        assert_classifier_signature(classify_future_send_attempt_result);
        assert_eq!(future_send_result_classifier_count(), 1);
    }

    #[test]
    fn future_send_result_diagnostic_is_redacted_and_state_machine_bound() {
        for outcome in future_send_outcomes() {
            let parts = ApprovedOrderEndpointRequestParts {
                operation: GatewayRealOrderEndpointOperation::CancelOrder,
                method_name: "DELETE",
                rendered_path: RenderedOrderEndpointPath(
                    "/v1/accounts/ACC_TEST_0001/orders/ORDER_TEST_0002".to_string(),
                ),
                approved_request_spec: ApprovedOrderEndpointRequestSpec::Cancel(
                    broker_finam::FinamCancelOrderRequestSpec {
                        account_id: "ACC_TEST_0001".to_string(),
                        order_id: "ORDER_TEST_0002".to_string(),
                    },
                ),
                account_instrument_allowlist_approved: true,
                operator_arm_approved: true,
                durable_state_checkpoint_present: true,
                durable_checkpoint_label:
                    GatewayRealOrderEndpointDurableCheckpointLabel::CancelRequestCancelPersistedBeforeEndpoint,
            };
            let diagnostic = future_send_result_redacted_diagnostic(parts, outcome);
            let rendered = serde_json::to_string(&diagnostic).expect("diagnostic serializes");

            assert!(!rendered.contains("/v1/accounts/ACC_TEST_0001"));
            assert!(!rendered.contains("ACC_TEST_0001"));
            assert!(!rendered.contains("ORDER_TEST_0002"));
            assert!(rendered.contains("\"endpoint_gate_required\":true"));
            assert!(rendered.contains("\"request_parts_consumed\":true"));
            assert!(rendered.contains("\"request_parts_reuse_after_outcome_allowed\":false"));
            assert!(rendered.contains("\"network_enabled\":false"));
            assert!(rendered.contains("\"rendered_path_redacted\":true"));
            assert!(rendered.contains("\"rendered_path_exported\":false"));
            assert!(rendered.contains("\"raw_body_exported\":false"));
            assert!(rendered.contains("\"retry_after_timeout_unknown_allowed\":false"));
            assert!(rendered.contains("\"state_machine_transition_required\":true"));
            assert!(rendered.contains("\"state_machine_bypass_allowed\":false"));
            assert!(rendered.contains("\"runtime_ack_redacted_only\":true"));
        }
    }

    #[test]
    fn diagnostics_cannot_feed_future_send_result_boundary() {
        let source = include_str!("real_order_endpoint.rs");
        let classifier_source = source
            .split("fn classify_future_send_attempt_result")
            .nth(1)
            .expect("classifier source")
            .split("fn approved_request_parts_constructor_count")
            .next()
            .expect("classifier boundary");

        assert!(classifier_source.contains("EndpointGateApproved"));
        assert!(classifier_source.contains("ApprovedOrderEndpointRequestParts"));
        assert!(classifier_source.contains("GatewayRealOrderEndpointFutureSendOutcome"));
        assert!(!classifier_source.contains("GatewayRealOrderEndpointRedactedRouteDiagnostic"));
        assert!(!classifier_source.contains("GatewayRealOrderEndpointApprovedPartsDiagnostic"));
        assert!(!classifier_source.contains("GatewayRealOrderEndpointConsumerDiagnostic"));
    }

    #[test]
    fn outcome_state_policy_matrix_covers_outcomes_and_redacted_ack_policy() {
        use GatewayRealOrderEndpointFutureSendOutcome as Outcome;
        use GatewayRealOrderEndpointOperatorPolicy as OperatorPolicy;

        let matrix = future_send_outcome_state_policy_matrix();
        let outcomes: Vec<_> = matrix.iter().map(|entry| entry.outcome).collect();
        assert_eq!(outcomes.as_slice(), future_send_outcomes().as_slice());

        for entry in &matrix {
            assert!(entry.state_machine_transition_required);
            assert!(!entry.result_diagnostic_can_bypass_state_machine);
            assert!(entry.runtime_ack_redacted_only);
        }

        let accepted = matrix
            .iter()
            .find(|entry| entry.outcome == Outcome::Accepted)
            .expect("accepted policy");
        assert_eq!(accepted.place_event, OrderPathEvent::SubmitAccepted);
        assert_eq!(accepted.place_state, OrderPathState::Submitted);
        assert_eq!(accepted.cancel_event, OrderPathEvent::CancelAccepted);
        assert_eq!(accepted.cancel_state, OrderPathState::CancelSubmitted);
        assert_eq!(accepted.ack_status, CommandAckStatus::Submitted);
        assert_eq!(accepted.place_ack_reason_code, None);
        assert_eq!(accepted.cancel_ack_reason_code, None);
        assert_eq!(accepted.operator_policy, OperatorPolicy::None);
        assert_eq!(accepted.operator_disarm_signal, None);
        assert!(!accepted.no_blind_retry);

        let rejected = matrix
            .iter()
            .find(|entry| entry.outcome == Outcome::Rejected)
            .expect("rejected policy");
        assert_eq!(rejected.place_state, OrderPathState::BrokerRejected);
        assert_eq!(
            rejected.error_kind,
            Some(OrderPathErrorKind::BrokerRejected)
        );
        assert_eq!(rejected.ack_status, CommandAckStatus::Rejected);
        assert_eq!(
            rejected.place_ack_reason_code,
            Some(CommandAckReasonCode::BrokerRejected)
        );
        assert_eq!(
            rejected.cancel_ack_reason_code,
            Some(CommandAckReasonCode::BrokerRejected)
        );

        let timeout = matrix
            .iter()
            .find(|entry| entry.outcome == Outcome::TimeoutUnknownPending)
            .expect("timeout policy");
        assert_eq!(timeout.place_event, OrderPathEvent::SubmitTimedOut);
        assert_eq!(timeout.place_state, OrderPathState::TimeoutUnknownPending);
        assert_eq!(timeout.cancel_event, OrderPathEvent::CancelTimedOut);
        assert_eq!(
            timeout.cancel_state,
            OrderPathState::CancelTimeoutUnknownPending
        );
        assert_eq!(timeout.ack_status, CommandAckStatus::Timeout);
        assert_eq!(
            timeout.place_ack_reason_code,
            Some(CommandAckReasonCode::TimeoutUnknownPending)
        );
        assert_eq!(
            timeout.cancel_ack_reason_code,
            Some(CommandAckReasonCode::CancelTimeoutUnknownPending)
        );
        assert_eq!(
            timeout.operator_disarm_signal,
            Some(OperatorDisarmSignal::UnknownPendingOrder)
        );
        assert!(timeout.manual_intervention_required);
        assert!(timeout.no_blind_retry);

        let rate_limited = matrix
            .iter()
            .find(|entry| entry.outcome == Outcome::RateLimited)
            .expect("rate-limit policy");
        assert_eq!(
            rate_limited.error_kind,
            Some(OrderPathErrorKind::RateLimited)
        );
        assert_eq!(rate_limited.ack_status, CommandAckStatus::Error);
        assert_eq!(
            rate_limited.operator_policy,
            OperatorPolicy::BackoffAndManualIntervention
        );
        assert_eq!(
            rate_limited.operator_disarm_signal,
            Some(OperatorDisarmSignal::OrderEndpointRateLimited)
        );
        assert!(rate_limited.backoff_required);
        assert!(rate_limited.manual_intervention_required);
        assert!(rate_limited.no_blind_retry);

        let maintenance = matrix
            .iter()
            .find(|entry| entry.outcome == Outcome::Maintenance)
            .expect("maintenance policy");
        assert_eq!(
            maintenance.error_kind,
            Some(OrderPathErrorKind::BrokerMaintenance)
        );
        assert_eq!(
            maintenance.operator_policy,
            OperatorPolicy::DegradeAndManualIntervention
        );
        assert_eq!(
            maintenance.operator_disarm_signal,
            Some(OperatorDisarmSignal::OrderEndpointMaintenance)
        );
        assert!(maintenance.manual_intervention_required);
        assert!(maintenance.no_blind_retry);

        let unauthorized = matrix
            .iter()
            .find(|entry| entry.outcome == Outcome::Unauthorized)
            .expect("unauthorized policy");
        assert_eq!(
            unauthorized.error_kind,
            Some(OrderPathErrorKind::Unauthorized)
        );
        assert_eq!(
            unauthorized.operator_disarm_signal,
            Some(OperatorDisarmSignal::OrderEndpointUnauthorized)
        );
        assert!(unauthorized.manual_intervention_required);
        assert!(unauthorized.no_blind_retry);

        let decode = matrix
            .iter()
            .find(|entry| entry.outcome == Outcome::DecodeError)
            .expect("decode policy");
        assert_eq!(
            decode.error_kind,
            Some(OrderPathErrorKind::ResponseDecodeError)
        );
        assert_eq!(
            decode.operator_policy,
            OperatorPolicy::DecodeManualIntervention
        );
        assert_eq!(
            decode.operator_disarm_signal,
            Some(OperatorDisarmSignal::OrderEndpointDecodeError)
        );
        assert!(decode.manual_intervention_required);
        assert!(decode.no_blind_retry);

        let transport = matrix
            .iter()
            .find(|entry| entry.outcome == Outcome::TransportError)
            .expect("transport policy");
        assert_eq!(transport.error_kind, Some(OrderPathErrorKind::Unknown));
        assert_eq!(
            transport.place_ack_reason_code,
            Some(CommandAckReasonCode::ManualInterventionRequired)
        );
        assert_eq!(
            transport.cancel_ack_reason_code,
            Some(CommandAckReasonCode::ManualInterventionRequired)
        );
        assert_eq!(
            transport.operator_policy,
            OperatorPolicy::TransportCategoryManualIntervention
        );
        assert_eq!(
            transport.operator_disarm_signal,
            Some(OperatorDisarmSignal::GatewayDegraded)
        );
        assert!(transport.backoff_required);
        assert!(transport.manual_intervention_required);
        assert!(transport.no_blind_retry);

        let rendered = serde_json::to_string(&matrix).expect("policy matrix serializes");
        assert!(!rendered.contains("ACC_TEST_0001"));
        assert!(!rendered.contains("ORDER_TEST_0001"));
        assert!(!rendered.contains("CID_TEST_0001"));
    }

    #[test]
    fn accepted_broker_id_policy_inherits_identity_reconciliation_policy() {
        use GatewayRealOrderEndpointAcceptedBrokerIdPolicy as Policy;

        let matrix = accepted_broker_id_policy_matrix();
        assert_eq!(matrix.len(), 4);

        for entry in &matrix {
            assert!(!entry.raw_broker_order_id_exported);
            assert!(entry.runtime_ack_redacted_only);
        }

        let accepted = matrix
            .iter()
            .find(|entry| entry.policy == Policy::AcceptedWithBrokerOrderId)
            .expect("accepted with broker id policy");
        assert_eq!(accepted.place_event, OrderPathEvent::SubmitAccepted);
        assert_eq!(accepted.place_state, OrderPathState::Submitted);
        assert_eq!(accepted.ack_status, CommandAckStatus::Submitted);
        assert_eq!(accepted.ack_reason_code, None);
        assert_eq!(accepted.operator_disarm_signal, None);
        assert!(!accepted.reconciliation_required);
        assert!(!accepted.no_blind_retry);
        assert!(!accepted.manual_intervention_required);

        let accepted_without = matrix
            .iter()
            .find(|entry| entry.policy == Policy::AcceptedWithoutBrokerOrderId)
            .expect("accepted without broker id policy");
        assert_eq!(
            accepted_without.place_event,
            OrderPathEvent::SubmitAcceptedWithoutBrokerOrderId
        );
        assert_eq!(
            accepted_without.place_state,
            OrderPathState::SubmittedPendingBrokerOrderId
        );
        assert_eq!(
            accepted_without.ack_status,
            CommandAckStatus::UnknownPending
        );
        assert_eq!(
            accepted_without.ack_reason_code,
            Some(CommandAckReasonCode::ReconciliationRequired)
        );
        assert_eq!(
            accepted_without.operator_disarm_signal,
            Some(OperatorDisarmSignal::AcceptedWithoutBrokerOrderId)
        );
        assert!(accepted_without.reconciliation_required);
        assert!(accepted_without.no_blind_retry);
        assert!(accepted_without.manual_intervention_required);

        let empty_broker_order_id = matrix
            .iter()
            .find(|entry| entry.policy == Policy::EmptyBrokerOrderIdDecodeError)
            .expect("empty broker order id policy");
        assert_eq!(
            empty_broker_order_id.place_event,
            OrderPathEvent::RequireManualIntervention
        );
        assert_eq!(
            empty_broker_order_id.ack_reason_code,
            Some(CommandAckReasonCode::ResponseDecodeError)
        );
        assert_eq!(
            empty_broker_order_id.operator_disarm_signal,
            Some(OperatorDisarmSignal::OrderEndpointDecodeError)
        );
        assert!(empty_broker_order_id.reconciliation_required);
        assert!(empty_broker_order_id.no_blind_retry);

        let mismatch = matrix
            .iter()
            .find(|entry| entry.policy == Policy::BrokerOrderIdMismatchManualIntervention)
            .expect("broker order id mismatch policy");
        assert_eq!(
            mismatch.place_event,
            OrderPathEvent::RequireManualIntervention
        );
        assert_eq!(mismatch.ack_status, CommandAckStatus::UnknownPending);
        assert_eq!(
            mismatch.ack_reason_code,
            Some(CommandAckReasonCode::ManualInterventionRequired)
        );
        assert_eq!(
            mismatch.operator_disarm_signal,
            Some(OperatorDisarmSignal::ReconciliationConflict)
        );
        assert!(mismatch.reconciliation_required);
        assert!(mismatch.no_blind_retry);
        assert!(mismatch.manual_intervention_required);
    }

    #[test]
    fn durable_checkpoint_capability_design_is_internal_and_operation_specific() {
        let shape = api_shape().durable_checkpoint_capability_design;
        assert!(shape.place_capability_type_internal);
        assert!(shape.cancel_capability_type_internal);
        assert!(shape.capability_not_debug_or_serializable);
        assert!(shape.created_after_sqlite_transition_only);
        assert_eq!(shape.place_required_event, OrderPathEvent::BeginSubmit);
        assert_eq!(
            shape.place_required_label,
            GatewayRealOrderEndpointDurableCheckpointLabel::PlaceBeginSubmitPersistedBeforeEndpoint
        );
        assert_eq!(shape.cancel_required_event, OrderPathEvent::RequestCancel);
        assert_eq!(
            shape.cancel_required_label,
            GatewayRealOrderEndpointDurableCheckpointLabel::CancelRequestCancelPersistedBeforeEndpoint
        );

        let source = include_str!("real_order_endpoint.rs");
        let place_marker = "PlaceEndpointDurableCheckpointApproved";
        let cancel_marker = "CancelEndpointDurableCheckpointApproved";
        assert!(source.contains(&format!("struct {place_marker}")));
        assert!(source.contains(&format!("struct {cancel_marker}")));
        assert!(!source.contains(&format!("pub struct {place_marker}")));
        assert!(!source.contains(&format!("pub struct {cancel_marker}")));
        assert!(
            !source.contains(
                "#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]\nstruct PlaceEndpointDurableCheckpointApproved"
            )
        );
        assert!(
            !source.contains(
                "#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]\nstruct CancelEndpointDurableCheckpointApproved"
            )
        );
        assert!(!source.contains(&format!("impl std::fmt::Debug for {place_marker}")));
        assert!(!source.contains(&format!("impl std::fmt::Debug for {cancel_marker}")));
    }

    #[test]
    fn outcome_diagnostics_cannot_bypass_state_machine_policy_matrix() {
        let source = include_str!("real_order_endpoint.rs");
        let matrix_source = source
            .split("pub fn future_send_outcome_state_policy_matrix")
            .nth(1)
            .expect("policy matrix source")
            .split("pub fn accepted_broker_id_policy_matrix")
            .next()
            .expect("policy matrix boundary");

        assert!(!matrix_source.contains("GatewayRealOrderEndpointFutureSendDiagnostic"));
        assert!(!matrix_source.contains("GatewayRealOrderEndpointConsumerDiagnostic"));
        assert!(matrix_source.contains("state_machine_transition_required: true"));
        assert!(matrix_source.contains("result_diagnostic_can_bypass_state_machine: false"));

        for entry in future_send_outcome_state_policy_matrix() {
            assert!(entry.state_machine_transition_required);
            assert!(!entry.result_diagnostic_can_bypass_state_machine);
        }
    }

    #[test]
    fn transport_category_policy_separates_timeout_from_non_timeout_failures() {
        use GatewayRealOrderEndpointFutureSendOutcome as Outcome;
        use GatewayRealOrderEndpointTransportCategory as Category;
        use GatewayRealOrderEndpointTransportStateSemantics as Semantics;

        let matrix = transport_category_policy_matrix();
        assert_eq!(matrix.len(), 5);
        assert_eq!(transport_category_count(), 5);

        let categories: Vec<_> = matrix.iter().map(|entry| entry.category).collect();
        assert_eq!(categories.as_slice(), transport_categories().as_slice());

        let timeout = matrix
            .iter()
            .find(|entry| entry.category == Category::Timeout)
            .expect("timeout category");
        assert_eq!(timeout.send_outcome, Outcome::TimeoutUnknownPending);
        assert_eq!(timeout.state_semantics, Semantics::TimeoutUnknownPending);
        assert_eq!(timeout.place_state, OrderPathState::TimeoutUnknownPending);
        assert_eq!(
            timeout.cancel_state,
            OrderPathState::CancelTimeoutUnknownPending
        );
        assert_eq!(
            timeout.error_kind,
            Some(OrderPathErrorKind::TransportTimeout)
        );
        assert_eq!(timeout.ack_status, CommandAckStatus::Timeout);
        assert_eq!(
            timeout.place_ack_reason_code,
            Some(CommandAckReasonCode::TimeoutUnknownPending)
        );
        assert_eq!(
            timeout.cancel_ack_reason_code,
            Some(CommandAckReasonCode::CancelTimeoutUnknownPending)
        );
        assert!(timeout.timeout_unknown_pending_semantics);
        assert!(!timeout.non_timeout_transport_semantics);
        assert!(timeout.reconciliation_required);
        assert!(timeout.no_blind_retry);

        for entry in matrix
            .iter()
            .filter(|entry| entry.category != Category::Timeout)
        {
            assert_eq!(entry.send_outcome, Outcome::TransportError);
            assert_eq!(entry.state_semantics, Semantics::NonTimeoutTransportFailure);
            assert_eq!(
                entry.place_state,
                OrderPathState::ManualInterventionRequired
            );
            assert_eq!(
                entry.cancel_state,
                OrderPathState::ManualInterventionRequired
            );
            assert_ne!(entry.error_kind, Some(OrderPathErrorKind::TransportTimeout));
            assert_ne!(entry.ack_status, CommandAckStatus::Timeout);
            assert_ne!(
                entry.place_ack_reason_code,
                Some(CommandAckReasonCode::TimeoutUnknownPending)
            );
            assert_ne!(
                entry.cancel_ack_reason_code,
                Some(CommandAckReasonCode::CancelTimeoutUnknownPending)
            );
            assert!(!entry.timeout_unknown_pending_semantics);
            assert!(entry.non_timeout_transport_semantics);
            assert!(entry.manual_intervention_required);
            assert!(entry.no_blind_retry);
            assert!(entry.state_machine_transition_required);
            assert!(!entry.result_diagnostic_can_bypass_state_machine);
            assert!(entry.runtime_ack_redacted_only);
        }

        let rendered = serde_json::to_string(&matrix).expect("transport policy matrix serializes");
        assert!(!rendered.contains("ACC_TEST_0001"));
        assert!(!rendered.contains("ORDER_TEST_0001"));
        assert!(!rendered.contains("CID_TEST_0001"));
    }

    #[test]
    fn accepted_result_classifier_policy_wires_broker_id_matrix() {
        use GatewayRealOrderEndpointAcceptedBrokerIdPolicy as Policy;
        use GatewayRealOrderEndpointAcceptedResultKind as Kind;
        use GatewayRealOrderEndpointFutureSendOutcome as Outcome;

        assert_eq!(accepted_result_kind_count(), 4);
        assert_eq!(accepted_result_kinds().len(), 4);
        let matrix = accepted_result_classifier_policy_matrix();
        assert_eq!(matrix.len(), accepted_broker_id_policy_matrix().len());

        for entry in &matrix {
            assert_eq!(entry.send_outcome, Outcome::Accepted);
            assert!(!entry.raw_broker_order_id_exported);
            assert!(entry.runtime_ack_redacted_only);
            assert!(entry.state_machine_transition_required);
            assert!(!entry.accepted_result_can_be_treated_as_unconditional_submitted);
        }

        let with_id = matrix
            .iter()
            .find(|entry| entry.kind == Kind::WithBrokerOrderId)
            .expect("with broker id accepted result");
        assert_eq!(with_id.inherited_policy, Policy::AcceptedWithBrokerOrderId);
        assert_eq!(with_id.place_event, OrderPathEvent::SubmitAccepted);
        assert_eq!(with_id.place_state, OrderPathState::Submitted);
        assert_eq!(with_id.ack_status, CommandAckStatus::Submitted);
        assert_eq!(with_id.ack_reason_code, None);
        assert!(!with_id.reconciliation_required);
        assert!(!with_id.no_blind_retry);

        let without_id = matrix
            .iter()
            .find(|entry| entry.kind == Kind::WithoutBrokerOrderId)
            .expect("without broker id accepted result");
        assert_eq!(
            without_id.inherited_policy,
            Policy::AcceptedWithoutBrokerOrderId
        );
        assert_eq!(
            without_id.place_event,
            OrderPathEvent::SubmitAcceptedWithoutBrokerOrderId
        );
        assert_eq!(
            without_id.place_state,
            OrderPathState::SubmittedPendingBrokerOrderId
        );
        assert_eq!(without_id.ack_status, CommandAckStatus::UnknownPending);
        assert_eq!(
            without_id.ack_reason_code,
            Some(CommandAckReasonCode::ReconciliationRequired)
        );
        assert_eq!(
            without_id.operator_disarm_signal,
            Some(OperatorDisarmSignal::AcceptedWithoutBrokerOrderId)
        );
        assert!(without_id.reconciliation_required);
        assert!(without_id.no_blind_retry);
        assert!(without_id.manual_intervention_required);

        let empty = matrix
            .iter()
            .find(|entry| entry.kind == Kind::EmptyBrokerOrderId)
            .expect("empty broker id accepted result");
        assert_eq!(
            empty.inherited_policy,
            Policy::EmptyBrokerOrderIdDecodeError
        );
        assert_eq!(empty.place_event, OrderPathEvent::RequireManualIntervention);
        assert_eq!(
            empty.ack_reason_code,
            Some(CommandAckReasonCode::ResponseDecodeError)
        );
        assert_eq!(
            empty.operator_disarm_signal,
            Some(OperatorDisarmSignal::OrderEndpointDecodeError)
        );
        assert!(empty.reconciliation_required);
        assert!(empty.no_blind_retry);

        let mismatch = matrix
            .iter()
            .find(|entry| entry.kind == Kind::BrokerOrderIdMismatch)
            .expect("broker id mismatch accepted result");
        assert_eq!(
            mismatch.inherited_policy,
            Policy::BrokerOrderIdMismatchManualIntervention
        );
        assert_eq!(
            mismatch.operator_disarm_signal,
            Some(OperatorDisarmSignal::ReconciliationConflict)
        );
        assert!(mismatch.reconciliation_required);
        assert!(mismatch.no_blind_retry);
        assert!(mismatch.manual_intervention_required);
    }

    #[test]
    fn accepted_result_classifier_requires_gate_parts_and_non_diagnostic_response_shape() {
        fn assert_classifier_signature(
            _f: fn(
                &EndpointGateApproved,
                ApprovedOrderEndpointRequestParts,
                GatewayRealOrderEndpointAcceptedResponseShape,
            ) -> GatewayRealOrderEndpointAcceptedResultDiagnostic,
        ) {
        }

        assert_classifier_signature(classify_accepted_result_after_future_send);
        assert_eq!(accepted_result_classifier_count(), 1);

        let source = include_str!("real_order_endpoint.rs");
        let classifier_source = source
            .split("fn classify_accepted_result_after_future_send")
            .nth(1)
            .expect("accepted classifier source")
            .split("fn approved_request_parts_constructor_count")
            .next()
            .expect("accepted classifier boundary");

        assert!(classifier_source.contains("EndpointGateApproved"));
        assert!(classifier_source.contains("ApprovedOrderEndpointRequestParts"));
        assert!(classifier_source.contains("GatewayRealOrderEndpointAcceptedResponseShape"));
        assert!(!classifier_source.contains("GatewayRealOrderEndpointFutureSendDiagnostic"));
        assert!(!classifier_source.contains("GatewayRealOrderEndpointConsumerDiagnostic"));
        assert!(!classifier_source.contains("GatewayRealOrderEndpointApprovedPartsDiagnostic"));
    }

    #[test]
    fn cancel_accepted_id_policy_covers_match_missing_and_mismatch() {
        use GatewayRealOrderEndpointCancelAcceptedIdPolicy as Policy;

        let matrix = cancel_accepted_id_policy_matrix();
        assert_eq!(matrix.len(), 3);

        for entry in &matrix {
            assert!(!entry.response_body_required);
            assert!(!entry.broker_order_id_required);
            assert!(entry.no_blind_retry);
            assert!(!entry.raw_broker_order_id_exported);
            assert!(entry.runtime_ack_redacted_only);
        }

        let matching = matrix
            .iter()
            .find(|entry| entry.policy == Policy::MatchingBrokerOrderId)
            .expect("matching cancel id policy");
        assert_eq!(matching.cancel_event, OrderPathEvent::CancelAccepted);
        assert_eq!(matching.cancel_state, OrderPathState::CancelSubmitted);
        assert_eq!(matching.ack_status, CommandAckStatus::Submitted);
        assert_eq!(matching.ack_reason_code, None);
        assert_eq!(matching.operator_disarm_signal, None);
        assert!(!matching.reconciliation_required);
        assert!(!matching.manual_intervention_required);

        let missing = matrix
            .iter()
            .find(|entry| entry.policy == Policy::MissingBrokerOrderIdAcceptedPendingReconciliation)
            .expect("missing cancel id policy");
        assert_eq!(missing.cancel_event, OrderPathEvent::CancelAccepted);
        assert_eq!(missing.cancel_state, OrderPathState::CancelSubmitted);
        assert_eq!(missing.ack_status, CommandAckStatus::UnknownPending);
        assert_eq!(
            missing.ack_reason_code,
            Some(CommandAckReasonCode::ReconciliationRequired)
        );
        assert_eq!(
            missing.operator_disarm_signal,
            Some(OperatorDisarmSignal::AcceptedWithoutBrokerOrderId)
        );
        assert!(missing.reconciliation_required);
        assert!(missing.manual_intervention_required);

        let mismatch = matrix
            .iter()
            .find(|entry| entry.policy == Policy::BrokerOrderIdMismatchManualIntervention)
            .expect("mismatched cancel id policy");
        assert_eq!(
            mismatch.cancel_event,
            OrderPathEvent::RequireManualIntervention
        );
        assert_eq!(
            mismatch.cancel_state,
            OrderPathState::ManualInterventionRequired
        );
        assert_eq!(mismatch.ack_status, CommandAckStatus::UnknownPending);
        assert_eq!(
            mismatch.ack_reason_code,
            Some(CommandAckReasonCode::ManualInterventionRequired)
        );
        assert_eq!(
            mismatch.operator_disarm_signal,
            Some(OperatorDisarmSignal::CancelBrokerOrderIdMismatch)
        );
        assert!(mismatch.reconciliation_required);
        assert!(mismatch.manual_intervention_required);
    }

    #[test]
    fn captured_envelope_design_redacts_raw_path_body_and_error() {
        fn assert_diagnostic_signature(
            _f: fn(
                &EndpointGateApproved,
                ApprovedOrderEndpointRequestParts,
                GatewayRealOrderEndpointCapturedEnvelopeKind,
                Option<GatewayRealOrderEndpointTransportCategory>,
            ) -> GatewayRealOrderEndpointCapturedResponseEnvelopeDiagnostic,
        ) {
        }

        assert_diagnostic_signature(captured_response_error_envelope_diagnostic);
        assert_eq!(captured_envelope_diagnostic_count(), 1);

        let matrix = captured_envelope_transport_category_matrix();
        assert_eq!(matrix.len(), transport_categories().len());
        for entry in &matrix {
            assert_eq!(
                entry.envelope_kind,
                GatewayRealOrderEndpointCapturedEnvelopeKind::TransportError
            );
            assert!(!entry.raw_error_exported);
            assert!(entry.transport_category_exported);
            assert!(entry.error_len_recorded);
            assert!(entry.error_sha256_recorded);
        }

        let diagnostic = GatewayRealOrderEndpointCapturedResponseEnvelopeDiagnostic {
            kind: GatewayRealOrderEndpointCapturedEnvelopeKind::TransportError,
            operation: GatewayRealOrderEndpointOperation::CancelOrder,
            method_name: "DELETE".to_string(),
            endpoint_gate_required: true,
            request_parts_consumed: true,
            request_snapshot_fingerprint_present: true,
            status_code_present: false,
            body_present: false,
            body_len_recorded: false,
            body_sha256_recorded: false,
            transport_category: Some(GatewayRealOrderEndpointTransportCategory::HttpSendError),
            error_len_recorded: true,
            error_sha256_recorded: true,
            raw_path_exported: false,
            raw_body_exported: false,
            raw_error_exported: false,
            runtime_ack_redacted_only: true,
        };
        assert_eq!(
            diagnostic.kind,
            GatewayRealOrderEndpointCapturedEnvelopeKind::TransportError
        );
        assert_eq!(
            diagnostic.operation,
            GatewayRealOrderEndpointOperation::CancelOrder
        );
        assert!(diagnostic.endpoint_gate_required);
        assert!(diagnostic.request_parts_consumed);
        assert!(diagnostic.request_snapshot_fingerprint_present);
        assert!(!diagnostic.status_code_present);
        assert!(!diagnostic.body_present);
        assert!(!diagnostic.body_len_recorded);
        assert!(!diagnostic.body_sha256_recorded);
        assert_eq!(
            diagnostic.transport_category,
            Some(GatewayRealOrderEndpointTransportCategory::HttpSendError)
        );
        assert!(diagnostic.error_len_recorded);
        assert!(diagnostic.error_sha256_recorded);
        assert!(!diagnostic.raw_path_exported);
        assert!(!diagnostic.raw_body_exported);
        assert!(!diagnostic.raw_error_exported);
        assert!(diagnostic.runtime_ack_redacted_only);

        let rendered = serde_json::to_string(&diagnostic).expect("diagnostic serializes");
        assert!(!rendered.contains("/v1/accounts/ACC_TEST_0001"));
        assert!(!rendered.contains("ACC_TEST_0001"));
        assert!(!rendered.contains("ORDER_TEST_0003"));
        let forbidden_error_token = ["req", "west"].join("");
        assert!(!rendered.contains(&forbidden_error_token));
        assert!(rendered.contains("\"raw_path_exported\":false"));
        assert!(rendered.contains("\"raw_body_exported\":false"));
        assert!(rendered.contains("\"raw_error_exported\":false"));
    }

    #[test]
    fn endpoint_attempt_journal_binds_parts_checkpoint_envelope_and_outcome() {
        fn assert_place_signature(
            _f: fn(
                &EndpointGateApproved,
                ApprovedOrderEndpointRequestParts,
                PlaceEndpointDurableCheckpointApproved,
                GatewayRealOrderEndpointCapturedEnvelopeRecord,
                GatewayRealOrderEndpointFutureSendOutcome,
            ) -> GatewayRealOrderEndpointAttemptDiagnostic,
        ) {
        }
        fn assert_cancel_signature(
            _f: fn(
                &EndpointGateApproved,
                ApprovedOrderEndpointRequestParts,
                CancelEndpointDurableCheckpointApproved,
                GatewayRealOrderEndpointCapturedEnvelopeRecord,
                GatewayRealOrderEndpointFutureSendOutcome,
            ) -> GatewayRealOrderEndpointAttemptDiagnostic,
        ) {
        }

        assert_place_signature(bind_place_endpoint_attempt_journal);
        assert_cancel_signature(bind_cancel_endpoint_attempt_journal);
        assert_eq!(endpoint_attempt_journal_binding_constructor_count(), 2);
        assert_eq!(endpoint_attempt_diagnostic_count(), 1);

        let diagnostic =
            endpoint_attempt_redacted_diagnostic(GatewayRealOrderEndpointOperation::PlaceOrder);
        assert_eq!(
            diagnostic.operation,
            GatewayRealOrderEndpointOperation::PlaceOrder
        );
        assert!(diagnostic.endpoint_attempt_id_hash_present);
        assert_eq!(diagnostic.endpoint_attempt_id_sha256_len, 64);
        assert!(diagnostic.request_snapshot_fingerprint_present);
        assert!(diagnostic.request_parts_bound);
        assert!(diagnostic.checkpoint_marker_bound);
        assert!(diagnostic.captured_envelope_bound);
        assert!(diagnostic.outcome_classifier_bound);
        assert!(diagnostic.state_machine_transition_required);
        assert!(!diagnostic.state_machine_bypass_allowed);
        assert!(!diagnostic.raw_path_exported);
        assert!(!diagnostic.raw_body_exported);
        assert!(!diagnostic.raw_error_exported);
        assert!(diagnostic.runtime_ack_redacted_only);

        let source = include_str!("real_order_endpoint.rs");
        let journal_source = source
            .split("fn bind_place_endpoint_attempt_journal")
            .nth(1)
            .expect("journal binding source")
            .split("fn approved_request_parts_constructor_count")
            .next()
            .expect("journal binding boundary");
        assert!(journal_source.contains("EndpointGateApproved"));
        assert!(journal_source.contains("ApprovedOrderEndpointRequestParts"));
        assert!(journal_source.contains("PlaceEndpointDurableCheckpointApproved"));
        assert!(journal_source.contains("CancelEndpointDurableCheckpointApproved"));
        assert!(journal_source.contains("GatewayRealOrderEndpointCapturedEnvelopeRecord"));
        assert!(journal_source.contains("GatewayRealOrderEndpointFutureSendOutcome"));
        assert!(
            !journal_source.contains("GatewayRealOrderEndpointCapturedResponseEnvelopeDiagnostic")
        );
        assert!(!journal_source.contains("GatewayRealOrderEndpointFutureSendDiagnostic"));
        assert!(!journal_source.contains("GatewayRealOrderEndpointAcceptedResultDiagnostic"));
    }

    #[test]
    fn durable_endpoint_attempt_journal_contract_binds_all_fingerprints() {
        fn assert_place_append_signature(
            _f: fn(
                &EndpointGateApproved,
                GatewayRealOrderEndpointDurableAttemptJournalAppendInput,
                PlaceEndpointDurableCheckpointApproved,
            ) -> GatewayRealOrderEndpointDurableAttemptJournalDiagnostic,
        ) {
        }
        fn assert_cancel_append_signature(
            _f: fn(
                &EndpointGateApproved,
                GatewayRealOrderEndpointDurableAttemptJournalAppendInput,
                CancelEndpointDurableCheckpointApproved,
            ) -> GatewayRealOrderEndpointDurableAttemptJournalDiagnostic,
        ) {
        }

        assert_place_append_signature(append_place_durable_endpoint_attempt_journal);
        assert_cancel_append_signature(append_cancel_durable_endpoint_attempt_journal);
        assert_eq!(durable_attempt_journal_append_constructor_count(), 2);
        assert_eq!(durable_attempt_journal_diagnostic_count(), 1);

        let diagnostic = durable_endpoint_attempt_journal_redacted_diagnostic(
            GatewayRealOrderEndpointOperation::CancelOrder,
        );
        assert_eq!(
            diagnostic.operation,
            GatewayRealOrderEndpointOperation::CancelOrder
        );
        assert!(diagnostic.endpoint_attempt_id_hash_present);
        assert_eq!(diagnostic.endpoint_attempt_id_sha256_len, 64);
        assert!(diagnostic.request_fingerprint_bound);
        assert!(diagnostic.checkpoint_proof_fingerprint_bound);
        assert!(diagnostic.captured_envelope_fingerprint_bound);
        assert!(diagnostic.outcome_fingerprint_bound);
        assert!(diagnostic.state_transition_result_fingerprint_bound);
        assert!(diagnostic.ack_diagnostic_fingerprint_bound);
        assert!(diagnostic.append_committed_after_state_transition);
        assert!(diagnostic.state_machine_transition_required);
        assert!(!diagnostic.state_machine_bypass_allowed);
        assert!(!diagnostic.raw_endpoint_attempt_id_exported);
        assert!(!diagnostic.raw_request_values_exported);
        assert!(!diagnostic.raw_broker_order_id_exported);
        assert!(!diagnostic.raw_path_exported);
        assert!(!diagnostic.raw_body_exported);
        assert!(!diagnostic.raw_error_exported);
        assert!(diagnostic.runtime_ack_redacted_only);

        let source = include_str!("real_order_endpoint.rs");
        for internal_type in [
            "GatewayRealOrderEndpointCheckpointProofFingerprint",
            "GatewayRealOrderEndpointCapturedEnvelopeFingerprint",
            "GatewayRealOrderEndpointOutcomeClassifierFingerprint",
            "GatewayRealOrderEndpointStateTransitionResultRecord",
            "GatewayRealOrderEndpointAckDiagnosticFingerprint",
            "GatewayRealOrderEndpointDurableAttemptJournalAppendInput",
            "GatewayRealOrderEndpointDurableAttemptJournalRecord",
        ] {
            assert!(source.contains(&format!("struct {internal_type}")));
            assert!(!source.contains(&format!("pub struct {internal_type}")));
            assert!(!source.contains(&format!("impl std::fmt::Debug for {internal_type}")));
        }

        let append_source = source
            .split("fn append_place_durable_endpoint_attempt_journal")
            .nth(1)
            .expect("durable journal append source")
            .split("fn approved_request_parts_constructor_count")
            .next()
            .expect("durable journal append boundary");
        assert!(append_source.contains("EndpointGateApproved"));
        assert!(append_source.contains("GatewayRealOrderEndpointDurableAttemptJournalAppendInput"));
        assert!(append_source.contains("PlaceEndpointDurableCheckpointApproved"));
        assert!(append_source.contains("CancelEndpointDurableCheckpointApproved"));
        assert!(
            !append_source.contains("GatewayRealOrderEndpointCapturedResponseEnvelopeDiagnostic")
        );
        assert!(!append_source.contains("GatewayRealOrderEndpointFutureSendDiagnostic"));
        assert!(!append_source.contains("GatewayRealOrderEndpointAcceptedResultDiagnostic"));
    }

    #[test]
    fn finam_status_semantics_records_documented_and_waiver_statuses() {
        use GatewayRealOrderEndpointFinamStatusBodyPolicy as BodyPolicy;
        use GatewayRealOrderEndpointFutureSendOutcome as Outcome;

        assert_eq!(documented_place_finam_rest_statuses(), [200, 400, 401, 429]);
        assert_eq!(
            documented_cancel_finam_rest_statuses(),
            [200, 400, 401, 404, 429]
        );

        let place = place_finam_status_semantics_matrix();
        let cancel = cancel_finam_status_semantics_matrix();
        let combined = finam_status_semantics_matrix();
        assert_eq!(place.len(), 7);
        assert_eq!(cancel.len(), 10);
        assert_eq!(combined.len(), 17);

        for entry in &combined {
            assert!(entry.no_blind_retry);
            assert!(entry.state_machine_transition_required);
            assert!(!entry.raw_path_exported);
            assert!(!entry.raw_body_exported);
            assert!(!entry.raw_error_exported);
            assert!(!entry.raw_broker_order_id_exported);
        }

        fn find_status_entry(
            matrix: &[GatewayRealOrderEndpointFinamStatusSemanticsEntry],
            status_code: u16,
        ) -> &GatewayRealOrderEndpointFinamStatusSemanticsEntry {
            matrix
                .iter()
                .find(|entry| entry.status_code == status_code)
                .expect("FINAM status semantics entry")
        }

        let place_ok = find_status_entry(&place, 200);
        assert!(place_ok.documented_by_finam_rest_docs);
        assert!(!place_ok.defensive_policy_only);
        assert!(!place_ok.implementation_gate_evidence_required);
        assert_eq!(
            place_ok.body_policy,
            BodyPolicy::PlaceSuccessBodyRequiredForSubmitted
        );
        assert_eq!(place_ok.future_send_outcome, Outcome::Accepted);
        assert_eq!(place_ok.order_path_event, OrderPathEvent::SubmitAccepted);
        assert_eq!(place_ok.order_path_state, OrderPathState::Submitted);
        assert_eq!(place_ok.ack_status, CommandAckStatus::Submitted);
        assert!(place_ok.body_required_for_immediate_submitted);

        for status_code in [201, 202, 204] {
            let place_undocumented = find_status_entry(&place, status_code);
            assert!(!place_undocumented.documented_by_finam_rest_docs);
            assert!(place_undocumented.defensive_policy_only);
            assert!(place_undocumented.implementation_gate_evidence_required);
            assert!(place_undocumented.waiver_required_before_live);
            assert_eq!(
                place_undocumented.body_policy,
                BodyPolicy::Undocumented2xxRequiresEvidenceOrWaiver
            );
            assert_eq!(place_undocumented.future_send_outcome, Outcome::DecodeError);
            assert_eq!(
                place_undocumented.order_path_state,
                OrderPathState::ManualInterventionRequired
            );
            assert!(place_undocumented.empty_body_reconciliation_required);
        }

        let cancel_ok = find_status_entry(&cancel, 200);
        assert!(cancel_ok.documented_by_finam_rest_docs);
        assert_eq!(cancel_ok.body_policy, BodyPolicy::CancelSuccessBodyOptional);
        assert_eq!(cancel_ok.future_send_outcome, Outcome::Accepted);
        assert_eq!(cancel_ok.order_path_event, OrderPathEvent::CancelAccepted);
        assert_eq!(cancel_ok.order_path_state, OrderPathState::CancelSubmitted);
        assert!(cancel_ok.body_optional_for_cancel_acceptance);

        let cancel_404 = find_status_entry(&cancel, 404);
        assert!(cancel_404.documented_by_finam_rest_docs);
        assert_eq!(
            cancel_404.body_policy,
            BodyPolicy::NotFoundRequiresReadOnlyReconciliation
        );
        assert_eq!(
            cancel_404.future_send_outcome,
            Outcome::TimeoutUnknownPending
        );
        assert_eq!(cancel_404.order_path_event, OrderPathEvent::CancelTimedOut);
        assert_eq!(
            cancel_404.order_path_state,
            OrderPathState::CancelTimeoutUnknownPending
        );
        assert_eq!(cancel_404.ack_status, CommandAckStatus::UnknownPending);
        assert_eq!(
            cancel_404.ack_reason_code,
            Some(CommandAckReasonCode::ReconciliationRequired)
        );
        assert!(cancel_404.cancel_reconciliation_required);

        for status_code in [409, 410] {
            let cancel_defensive = find_status_entry(&cancel, status_code);
            assert!(!cancel_defensive.documented_by_finam_rest_docs);
            assert!(cancel_defensive.defensive_policy_only);
            assert!(cancel_defensive.implementation_gate_evidence_required);
            assert!(cancel_defensive.waiver_required_before_live);
            assert!(cancel_defensive.cancel_reconciliation_required);
            assert_eq!(
                cancel_defensive.order_path_state,
                OrderPathState::CancelTimeoutUnknownPending
            );
        }

        let place_400 = find_status_entry(&place, 400);
        assert_eq!(place_400.future_send_outcome, Outcome::Rejected);
        assert_eq!(place_400.ack_status, CommandAckStatus::Rejected);
        assert_eq!(
            place_400.ack_reason_code,
            Some(CommandAckReasonCode::BrokerRejected)
        );

        let rendered = serde_json::to_string(&combined).expect("status semantics serializes");
        assert!(!rendered.contains("ACC_TEST_0001"));
        assert!(!rendered.contains("ORDER_TEST_0001"));
        assert!(!rendered.contains("/v1/accounts/ACC_TEST_0001"));
    }

    #[test]
    fn implementation_gate_evidence_closure_plan_keeps_order_calls_closed() {
        use GatewayRealOrderEndpointEvidenceClosureMethod as Method;
        use GatewayRealOrderEndpointEvidenceClosureStatus as Status;
        use GatewayRealOrderEndpointEvidenceSlot as Slot;

        let plan = implementation_gate_evidence_closure_plan();
        assert_eq!(plan.len(), 5);

        for entry in &plan {
            assert_eq!(
                entry.current_status,
                Status::PendingBeforeImplementationGate
            );
            assert!(entry.must_close_before_implementation_gate);
            assert!(!entry.endpoint_calls_allowed_for_closure);
            assert!(!entry.order_endpoint_calls_allowed_for_closure);
            assert!(entry.reviewer_acceptance_required);
            assert!(entry.artifact_redacted_only);
            assert!(entry.source_archive_binding_required);
            assert!(!entry.raw_secret_exported);
            assert!(!entry.raw_account_exported);
            assert!(!entry.raw_order_id_exported);
            assert!(!entry.raw_path_exported);
            assert!(!entry.raw_body_exported);
        }

        let release = plan
            .iter()
            .find(|entry| entry.slot == Slot::ReleaseProfileEvidenceOrWaiver)
            .expect("release profile slot");
        assert_eq!(
            release.accepted_closure_methods,
            vec![Method::ControlledEvidence, Method::ReviewerAcceptedWaiver]
        );

        let get_order = plan
            .iter()
            .find(|entry| entry.slot == Slot::PositiveGetOrderEvidenceOrWaiver)
            .expect("positive get-order slot");
        assert!(get_order
            .accepted_closure_methods
            .contains(&Method::ControlledEvidence));
        assert!(get_order
            .accepted_closure_methods
            .contains(&Method::ReviewerAcceptedWaiver));

        let route_recheck = plan
            .iter()
            .find(|entry| entry.slot == Slot::RouteTemplateRecheck)
            .expect("route-template slot");
        assert_eq!(
            route_recheck.accepted_closure_methods,
            vec![
                Method::OfficialDocsConfirmation,
                Method::ReviewerAcceptedWaiver
            ]
        );

        for slot in [
            Slot::Undocumented2xxStatusSemantics,
            Slot::Cancel409410StatusSemantics,
        ] {
            let status_policy = plan
                .iter()
                .find(|entry| entry.slot == slot)
                .expect("status policy slot");
            assert!(status_policy
                .accepted_closure_methods
                .contains(&Method::ControlledEvidence));
            assert!(status_policy
                .accepted_closure_methods
                .contains(&Method::OfficialDocsConfirmation));
            assert!(status_policy
                .accepted_closure_methods
                .contains(&Method::ReviewerAcceptedWaiver));
        }

        let rendered = serde_json::to_string(&plan).expect("evidence plan serializes");
        assert!(!rendered.contains("ACC_TEST_0001"));
        assert!(!rendered.contains("ORDER_TEST_0001"));
        assert!(!rendered.contains("Bearer "));
    }

    #[test]
    fn durable_attempt_journal_sqlite_schema_design_is_hash_only_and_replay_safe() {
        use GatewayRealOrderEndpointDurableAttemptJournalColumnKind as ColumnKind;
        use GatewayRealOrderEndpointDurableAttemptJournalReplayDecision as Decision;

        let columns = durable_attempt_journal_sqlite_columns();
        assert_eq!(columns.len(), 19);
        assert!(columns.iter().all(|column| !column.stores_raw_value));

        let attempt_id = columns
            .iter()
            .find(|column| column.name == "endpoint_attempt_id_sha256")
            .expect("endpoint attempt id hash column");
        assert_eq!(attempt_id.kind, ColumnKind::PrimaryKeyHash);
        assert!(attempt_id.required);
        assert!(attempt_id.unique);

        let broker_order_id = columns
            .iter()
            .find(|column| column.name == "broker_order_id_sha256")
            .expect("optional broker id hash column");
        assert_eq!(broker_order_id.kind, ColumnKind::Sha256);
        assert!(!broker_order_id.required);
        assert!(!broker_order_id.unique);

        for required_hash in [
            "request_fingerprint_sha256",
            "checkpoint_proof_sha256",
            "captured_envelope_sha256",
            "outcome_sha256",
            "state_transition_sha256",
            "ack_diagnostic_sha256",
            "replay_fingerprint_set_sha256",
        ] {
            let column = columns
                .iter()
                .find(|column| column.name == required_hash)
                .expect("required hash column");
            assert_eq!(column.kind, ColumnKind::Sha256);
            assert!(column.required);
            assert!(!column.unique);
        }

        let indexes = durable_attempt_journal_sqlite_indexes();
        assert_eq!(indexes.len(), 4);
        assert_eq!(indexes.iter().filter(|index| index.unique).count(), 1);
        assert!(indexes.iter().any(|index| {
            index.name == "ux_order_endpoint_attempts_attempt_id"
                && index.unique
                && index.columns == vec!["endpoint_attempt_id_sha256"]
        }));
        assert!(indexes.iter().any(|index| {
            index.name == "ix_order_endpoint_attempts_replay_set"
                && index.replay_policy_related
                && index.columns
                    == vec![
                        "endpoint_attempt_id_sha256".to_string(),
                        "replay_fingerprint_set_sha256".to_string(),
                    ]
        }));

        let replay = durable_attempt_journal_replay_policy_matrix();
        assert_eq!(replay.len(), 2);

        let same = replay
            .iter()
            .find(|entry| entry.decision == Decision::SameFingerprintSetIdempotentReplay)
            .expect("same fingerprint replay");
        assert!(same.endpoint_attempt_id_hash_match_required);
        assert!(same.full_fingerprint_set_match_required);
        assert!(same.state_transition_result_match_required);
        assert!(same.ack_diagnostic_match_required);
        assert!(!same.raw_values_compared);
        assert!(same.no_blind_retry);
        assert!(!same.operator_disarm_on_conflict);

        let conflict = replay
            .iter()
            .find(|entry| entry.decision == Decision::DifferentFingerprintSetRejectAndDisarm)
            .expect("conflict replay");
        assert!(conflict.endpoint_attempt_id_hash_match_required);
        assert!(!conflict.full_fingerprint_set_match_required);
        assert!(!conflict.raw_values_compared);
        assert!(conflict.no_blind_retry);
        assert!(conflict.operator_disarm_on_conflict);

        let rendered =
            serde_json::to_string(&(columns, indexes, replay)).expect("schema design serializes");
        assert!(!rendered.contains("ACC_TEST_0001"));
        assert!(!rendered.contains("ORDER_TEST_0001"));
        assert!(!rendered.contains("/v1/accounts/ACC_TEST_0001"));
    }

    #[test]
    fn durable_journal_migration_runbook_records_startup_and_operator_disarm_policy() {
        use GatewayRealOrderEndpointDurableJournalMigrationStepKind as Step;

        let steps = durable_journal_migration_runbook_steps();
        assert_eq!(steps.len(), 9);

        let expected = [
            Step::BackupBeforeMigration,
            Step::AcquireSingleWriterLock,
            Step::OpenSqliteWithWal,
            Step::SetSynchronousFull,
            Step::VerifySchemaVersion,
            Step::CreateOrderEndpointAttemptsTable,
            Step::CreateReplayIndexes,
            Step::RunIntegrityCheck,
            Step::RefuseAutoRepair,
        ];

        for (entry, expected_step) in steps.iter().zip(expected) {
            assert_eq!(entry.step, expected_step);
            assert!(entry.required_before_endpoint_gate);
            assert!(entry.operator_visible);
            assert!(entry.failure_disarms_order_endpoints);
            assert!(!entry.raw_values_exported);
        }

        let shape = api_shape().durable_journal_migration_runbook_design;
        assert!(shape.migration_runbook_design_only);
        assert_eq!(shape.step_count, expected.len());
        assert!(shape.backup_required_before_migration);
        assert!(shape.begin_immediate_required_for_schema_change);
        assert!(shape.wal_required);
        assert!(shape.synchronous_full_required);
        assert!(shape.single_writer_lock_required);
        assert!(shape.schema_version_guard_required);
        assert!(shape.sqlite_integrity_check_required);
        assert!(shape.corruption_open_failure_disarms);
        assert!(shape.stale_or_unknown_lock_disarms);
        assert!(!shape.auto_repair_allowed);
        assert!(!shape.auto_stale_lock_delete_allowed);
        assert!(shape.operator_runbook_required);
        assert!(shape.redacted_operator_diagnostics_only);
        assert!(!shape.raw_sqlite_path_exported);
        assert!(!shape.raw_request_values_exported);
        assert!(!shape.raw_broker_payload_exported);

        let rendered = serde_json::to_string(&(shape, steps)).expect("runbook serializes");
        assert!(!rendered.contains("ACC_TEST_0001"));
        assert!(!rendered.contains("ORDER_TEST_0001"));
        assert!(!rendered.contains("Bearer "));
    }

    #[test]
    fn canonical_replay_fingerprint_spec_has_stable_order_and_no_raw_values() {
        use GatewayRealOrderEndpointCanonicalReplayFingerprintField as Field;

        let fields = canonical_replay_fingerprint_fields();
        let expected = [
            Field::SchemaVersion,
            Field::Operation,
            Field::EndpointAttemptIdSha256,
            Field::RequestIdSha256,
            Field::ClientOrderIdSha256,
            Field::AccountSha256,
            Field::InstrumentSha256,
            Field::CheckpointLabel,
            Field::RequestFingerprintSha256,
            Field::CheckpointProofSha256,
            Field::CapturedEnvelopeSha256,
            Field::OutcomeSha256,
            Field::StateTransitionSha256,
            Field::AckDiagnosticSha256,
        ];

        assert_eq!(fields.len(), expected.len());

        for (index, entry) in fields.iter().enumerate() {
            assert_eq!(entry.ordinal, index + 1);
            assert_eq!(entry.field, expected[index]);
            assert!(entry.required);
            assert_eq!(entry.hash_len, 64);
            assert!(!entry.raw_value_exported);
        }

        let shape = api_shape().canonical_replay_fingerprint_design;
        assert!(shape.fingerprint_spec_design_only);
        assert_eq!(shape.field_count, expected.len());
        assert_eq!(
            shape.encoding,
            GatewayRealOrderEndpointCanonicalReplayEncoding::Utf8JsonObjectSortedKeysNoWhitespace
        );
        assert!(shape.schema_version_included);
        assert!(shape.operation_included);
        assert!(shape.endpoint_attempt_id_included);
        assert!(shape.request_client_account_instrument_hashes_included);
        assert!(shape.checkpoint_and_envelope_hashes_included);
        assert!(shape.outcome_state_ack_hashes_included);
        assert!(shape.stable_field_order_required);
        assert!(shape.sorted_keys_required);
        assert!(shape.whitespace_forbidden);
        assert_eq!(shape.sha256_len, 64);
        assert!(!shape.raw_values_exported);
        assert!(shape.refactor_changes_require_schema_bump);

        let rendered = serde_json::to_string(&(shape, fields)).expect("fingerprint serializes");
        assert!(!rendered.contains("ACC_TEST_0001"));
        assert!(!rendered.contains("ORDER_TEST_0001"));
        assert!(!rendered.contains("Bearer "));
    }

    #[test]
    fn endpoint_attempt_id_lifecycle_prevents_reuse_after_timeout_manual_or_terminal() {
        use GatewayRealOrderEndpointAttemptIdLifecyclePhase as Phase;

        let policy = endpoint_attempt_id_lifecycle_policy();
        let expected = [
            (Phase::GeneratedAfterApprovedRequestParts, true),
            (Phase::BoundBeforeEndpointSend, true),
            (Phase::PersistedWithAttemptJournal, true),
            (Phase::ReusedOnlyForIdempotentReplay, false),
            (Phase::NeverReusedForNewAttemptAfterTerminalOrManual, false),
        ];

        assert_eq!(policy.len(), expected.len());

        for (entry, (expected_phase, network_allowed)) in policy.iter().zip(expected) {
            assert_eq!(entry.phase, expected_phase);
            assert!(entry.requires_endpoint_gate);
            assert!(entry.requires_request_id_hash);
            assert!(entry.requires_client_order_id_hash);
            assert!(entry.requires_operation);
            assert_eq!(entry.endpoint_attempt_id_sha256_len, 64);
            assert_eq!(entry.new_network_attempt_allowed, network_allowed);
            assert!(!entry.raw_endpoint_attempt_id_exported);
        }

        let shape = api_shape().endpoint_attempt_id_lifecycle_design;
        assert!(shape.lifecycle_design_only);
        assert_eq!(shape.phase_count, expected.len());
        assert!(shape.generated_after_approved_request_parts);
        assert!(shape.generated_before_future_endpoint_send);
        assert!(shape.bound_to_request_and_client_id_hashes);
        assert!(shape.bound_to_operation);
        assert!(shape.persisted_before_outcome_export);
        assert!(shape.same_attempt_id_replay_requires_same_fingerprint_set);
        assert!(!shape.reuse_after_timeout_manual_or_terminal_allowed);
        assert!(shape.new_endpoint_attempt_requires_new_id);
        assert!(!shape.raw_endpoint_attempt_id_exported);

        let rendered = serde_json::to_string(&(shape, policy)).expect("lifecycle serializes");
        assert!(!rendered.contains("ACC_TEST_0001"));
        assert!(!rendered.contains("ORDER_TEST_0001"));
        assert!(!rendered.contains("Bearer "));
    }

    #[test]
    fn implementation_gate_readiness_checklist_records_pending_evidence_without_endpoint_calls() {
        use GatewayRealOrderEndpointImplementationGateReadinessItem as Item;
        use GatewayRealOrderEndpointImplementationGateReadinessStatus as Status;

        let checklist = implementation_gate_readiness_checklist();
        assert_eq!(checklist.len(), 12);

        for entry in &checklist {
            assert!(entry.must_close_before_implementation_gate);
            assert!(entry.reviewer_acceptance_required);
            assert!(!entry.endpoint_calls_allowed_for_check);
            assert!(!entry.raw_values_exported);
        }

        let implemented = checklist
            .iter()
            .filter(|entry| entry.status == Status::ImplementedAndTested)
            .count();
        let pending = checklist
            .iter()
            .filter(|entry| entry.status == Status::PendingEvidenceOrWaiver)
            .count();
        let design_recorded = checklist
            .iter()
            .filter(|entry| entry.status == Status::DesignRecorded)
            .count();

        assert_eq!(implemented, 7);
        assert_eq!(pending, 3);
        assert_eq!(design_recorded, 2);

        for item in [
            Item::ReleaseProfileEvidenceOrWaiver,
            Item::PositiveGetOrderEvidenceOrWaiver,
            Item::RouteTemplateRecheck,
        ] {
            let entry = checklist
                .iter()
                .find(|entry| entry.item == item)
                .expect("pending evidence item");
            assert_eq!(entry.status, Status::PendingEvidenceOrWaiver);
        }

        for item in [
            Item::CanonicalReplayGoldenVectors,
            Item::OperatorReplayRunbook,
        ] {
            let entry = checklist
                .iter()
                .find(|entry| entry.item == item)
                .expect("design recorded item");
            assert_eq!(entry.status, Status::DesignRecorded);
        }

        let rendered = serde_json::to_string(&checklist).expect("checklist serializes");
        assert!(!rendered.contains("ACC_TEST_0001"));
        assert!(!rendered.contains("ORDER_TEST_0001"));
        assert!(!rendered.contains("Bearer "));
    }

    #[test]
    fn canonical_replay_golden_vector_locks_json_and_sha256() {
        let vectors = canonical_replay_golden_vectors();
        assert_eq!(vectors.len(), 1);

        let vector = &vectors[0];
        assert_eq!(
            vector.name,
            "place_order_schema_v1_sorted_keys_no_whitespace"
        );
        assert_eq!(vector.schema_version, 1);
        assert_eq!(
            vector.encoding,
            GatewayRealOrderEndpointCanonicalReplayEncoding::Utf8JsonObjectSortedKeysNoWhitespace
        );
        assert_eq!(
            vector.field_count,
            canonical_replay_fingerprint_fields().len()
        );
        assert!(vector.no_whitespace);
        assert!(vector.sorted_keys_required);
        assert!(!vector.raw_values_exported);
        assert_eq!(vector.expected_sha256.len(), 64);
        assert_eq!(
            vector.expected_sha256,
            "d467afd3b7d320c26966a1a400995e00664397ed47bb74320a418cfd2524abc6"
        );
        assert_eq!(
            sha256_hex(vector.canonical_json.as_bytes()),
            vector.expected_sha256
        );
        assert_eq!(
            vector.canonical_json,
            concat!(
                "{\"account_sha256\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",",
                "\"ack_diagnostic_sha256\":\"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\",",
                "\"captured_envelope_sha256\":\"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\",",
                "\"checkpoint_label\":\"PlaceBeginSubmitPersistedBeforeEndpoint\",",
                "\"checkpoint_proof_sha256\":\"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd\",",
                "\"client_order_id_sha256\":\"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee\",",
                "\"endpoint_attempt_id_sha256\":\"1111111111111111111111111111111111111111111111111111111111111111\",",
                "\"instrument_sha256\":\"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\",",
                "\"operation\":\"PlaceOrder\",",
                "\"outcome_sha256\":\"9999999999999999999999999999999999999999999999999999999999999999\",",
                "\"request_fingerprint_sha256\":\"7777777777777777777777777777777777777777777777777777777777777777\",",
                "\"request_id_sha256\":\"2222222222222222222222222222222222222222222222222222222222222222\",",
                "\"schema_version\":1,",
                "\"state_transition_sha256\":\"8888888888888888888888888888888888888888888888888888888888888888\"}"
            )
        );
        assert!(!vector.canonical_json.chars().any(char::is_whitespace));
        assert!(!vector.canonical_json.contains("ACC_TEST_0001"));
        assert!(!vector.canonical_json.contains("ORDER_TEST_0001"));
        assert!(!vector.canonical_json.contains("Bearer "));
    }

    #[test]
    fn operator_replay_runbook_links_lifecycle_to_redacted_operator_actions() {
        use GatewayRealOrderEndpointOperatorReplayRunbookCase as Case;

        let entries = operator_replay_runbook_entries();
        assert_eq!(entries.len(), 5);

        for entry in &entries {
            assert!(entry.operator_visible);
            assert!(entry.redacted_diagnostic_only);
            assert!(entry.no_blind_retry);
            assert!(!entry.raw_endpoint_attempt_id_exported);
        }

        let idempotent = entries
            .iter()
            .find(|entry| entry.case == Case::IdempotentReplaySameFingerprint)
            .expect("idempotent replay case");
        assert!(idempotent.same_endpoint_attempt_id_allowed);
        assert!(!idempotent.new_endpoint_attempt_id_required);
        assert!(!idempotent.disarm_required);

        for case in [
            Case::ConflictingReplayDisarm,
            Case::TimeoutUnknownPending,
            Case::ManualIntervention,
        ] {
            let entry = entries
                .iter()
                .find(|entry| entry.case == case)
                .expect("operator disarm case");
            assert!(!entry.same_endpoint_attempt_id_allowed);
            assert!(entry.new_endpoint_attempt_id_required);
            assert!(entry.disarm_required);
        }

        let terminal = entries
            .iter()
            .find(|entry| entry.case == Case::TerminalOutcomeNewAttempt)
            .expect("terminal outcome case");
        assert!(!terminal.same_endpoint_attempt_id_allowed);
        assert!(terminal.new_endpoint_attempt_id_required);
        assert!(!terminal.disarm_required);

        let rendered = serde_json::to_string(&entries).expect("runbook serializes");
        assert!(!rendered.contains("ACC_TEST_0001"));
        assert!(!rendered.contains("ORDER_TEST_0001"));
        assert!(!rendered.contains("Bearer "));
    }

    #[test]
    fn evidence_closure_package_keeps_all_slots_pending_and_order_calls_closed() {
        use GatewayRealOrderEndpointEvidenceClosurePackageStatus as Status;
        use GatewayRealOrderEndpointEvidenceSlot as Slot;

        let entries = evidence_closure_package_entries();
        assert_eq!(entries.len(), 5);

        for slot in [
            Slot::ReleaseProfileEvidenceOrWaiver,
            Slot::PositiveGetOrderEvidenceOrWaiver,
            Slot::RouteTemplateRecheck,
            Slot::Undocumented2xxStatusSemantics,
            Slot::Cancel409410StatusSemantics,
        ] {
            let entry = entries
                .iter()
                .find(|entry| entry.slot == slot)
                .expect("closure package slot");
            assert_eq!(entry.status, Status::PendingEvidenceOrWaiver);
            assert!(entry.evidence_or_waiver_required);
            assert!(entry.reviewer_acceptance_required);
            assert!(entry.source_archive_binding_required);
            assert!(!entry.order_endpoint_calls_allowed_for_closure);
            assert!(!entry.raw_values_exported);
        }

        let rendered = serde_json::to_string(&entries).expect("closure package serializes");
        assert!(!rendered.contains("ACC_TEST_0001"));
        assert!(!rendered.contains("ORDER_TEST_0001"));
        assert!(!rendered.contains("Bearer "));
    }

    #[test]
    fn http_status_outcome_matrix_covers_place_and_cancel_statuses() {
        use GatewayRealOrderEndpointCancelAcceptedIdPolicy as CancelPolicy;
        use GatewayRealOrderEndpointCapturedEnvelopeKind as EnvelopeKind;
        use GatewayRealOrderEndpointFutureSendOutcome as Outcome;
        use GatewayRealOrderEndpointHttpBodyShape as Body;

        let place = place_http_status_outcome_matrix();
        let cancel = cancel_http_status_outcome_matrix();
        let combined = http_status_outcome_matrix();
        assert_eq!(place.len(), 15);
        assert_eq!(cancel.len(), 15);
        assert_eq!(combined.len(), 30);

        for entry in &combined {
            assert!(entry.state_machine_transition_required);
            assert!(entry.captured_envelope_required);
            assert!(entry.endpoint_attempt_journal_required);
            assert!(entry.no_blind_retry);
            assert!(!entry.raw_path_exported);
            assert!(!entry.raw_body_exported);
            assert!(!entry.raw_error_exported);
        }

        fn find_status_entry(
            matrix: &[GatewayRealOrderEndpointHttpStatusOutcomeEntry],
            status_code: u16,
            body_shape: GatewayRealOrderEndpointHttpBodyShape,
        ) -> &GatewayRealOrderEndpointHttpStatusOutcomeEntry {
            matrix
                .iter()
                .find(|entry| entry.status_code == status_code && entry.body_shape == body_shape)
                .expect("status/body-shape entry")
        }

        let place_accepted = find_status_entry(&place, 200, Body::AcceptedWithBrokerOrderId);
        assert_eq!(place_accepted.outcome, Outcome::Accepted);
        assert_eq!(
            place_accepted.accepted_result_kind,
            Some(GatewayRealOrderEndpointAcceptedResultKind::WithBrokerOrderId)
        );
        assert_eq!(
            place_accepted.order_path_event,
            OrderPathEvent::SubmitAccepted
        );
        assert_eq!(place_accepted.order_path_state, OrderPathState::Submitted);
        assert_eq!(place_accepted.ack_status, CommandAckStatus::Submitted);

        let place_missing_id = find_status_entry(&place, 200, Body::AcceptedWithoutBrokerOrderId);
        assert_eq!(
            place_missing_id.accepted_result_kind,
            Some(GatewayRealOrderEndpointAcceptedResultKind::WithoutBrokerOrderId)
        );
        assert_eq!(
            place_missing_id.order_path_event,
            OrderPathEvent::SubmitAcceptedWithoutBrokerOrderId
        );
        assert_eq!(
            place_missing_id.order_path_state,
            OrderPathState::SubmittedPendingBrokerOrderId
        );
        assert_eq!(
            place_missing_id.ack_status,
            CommandAckStatus::UnknownPending
        );

        let cancel_accepted = find_status_entry(&cancel, 200, Body::AcceptedWithBrokerOrderId);
        assert_eq!(
            cancel_accepted.cancel_accepted_id_policy,
            Some(CancelPolicy::MatchingBrokerOrderId)
        );
        assert_eq!(
            cancel_accepted.order_path_event,
            OrderPathEvent::CancelAccepted
        );
        assert_eq!(
            cancel_accepted.order_path_state,
            OrderPathState::CancelSubmitted
        );

        let cancel_missing = find_status_entry(&cancel, 200, Body::AcceptedWithoutBrokerOrderId);
        assert_eq!(
            cancel_missing.cancel_accepted_id_policy,
            Some(CancelPolicy::MissingBrokerOrderIdAcceptedPendingReconciliation)
        );
        assert_eq!(cancel_missing.ack_status, CommandAckStatus::UnknownPending);
        assert_eq!(
            cancel_missing.ack_reason_code,
            Some(CommandAckReasonCode::ReconciliationRequired)
        );

        let cancel_mismatch = find_status_entry(&cancel, 200, Body::AcceptedBrokerOrderIdMismatch);
        assert_eq!(
            cancel_mismatch.cancel_accepted_id_policy,
            Some(CancelPolicy::BrokerOrderIdMismatchManualIntervention)
        );
        assert_eq!(
            cancel_mismatch.operator_disarm_signal,
            Some(OperatorDisarmSignal::CancelBrokerOrderIdMismatch)
        );

        for status in [400, 422] {
            let place_reject = find_status_entry(&place, status, Body::BrokerReject);
            assert_eq!(
                place_reject.envelope_kind,
                EnvelopeKind::BrokerErrorResponse
            );
            assert_eq!(place_reject.outcome, Outcome::Rejected);
            assert_eq!(
                place_reject.order_path_state,
                OrderPathState::BrokerRejected
            );

            let cancel_reject = find_status_entry(&cancel, status, Body::BrokerReject);
            assert_eq!(cancel_reject.outcome, Outcome::Rejected);
            assert_eq!(
                cancel_reject.order_path_state,
                OrderPathState::ManualInterventionRequired
            );
        }

        for status in [401, 403] {
            let unauthorized = find_status_entry(&place, status, Body::Unauthorized);
            assert_eq!(unauthorized.outcome, Outcome::Unauthorized);
            assert_eq!(
                unauthorized.operator_disarm_signal,
                Some(OperatorDisarmSignal::OrderEndpointUnauthorized)
            );
        }

        for status in [408, 504] {
            let place_timeout = find_status_entry(&place, status, Body::Timeout);
            assert_eq!(place_timeout.outcome, Outcome::TimeoutUnknownPending);
            assert_eq!(
                place_timeout.ack_reason_code,
                Some(CommandAckReasonCode::TimeoutUnknownPending)
            );
            assert_eq!(
                place_timeout.order_path_state,
                OrderPathState::TimeoutUnknownPending
            );

            let cancel_timeout = find_status_entry(&cancel, status, Body::Timeout);
            assert_eq!(cancel_timeout.outcome, Outcome::TimeoutUnknownPending);
            assert_eq!(
                cancel_timeout.ack_reason_code,
                Some(CommandAckReasonCode::CancelTimeoutUnknownPending)
            );
            assert_eq!(
                cancel_timeout.order_path_state,
                OrderPathState::CancelTimeoutUnknownPending
            );
        }

        let rate_limited = find_status_entry(&place, 429, Body::RateLimit);
        assert_eq!(rate_limited.outcome, Outcome::RateLimited);
        assert_eq!(
            rate_limited.operator_disarm_signal,
            Some(OperatorDisarmSignal::OrderEndpointRateLimited)
        );

        for status in [500, 502, 503] {
            let maintenance = find_status_entry(&cancel, status, Body::Maintenance);
            assert_eq!(maintenance.outcome, Outcome::Maintenance);
            assert_eq!(
                maintenance.operator_disarm_signal,
                Some(OperatorDisarmSignal::OrderEndpointMaintenance)
            );
        }

        let malformed = find_status_entry(&place, 200, Body::MalformedBody);
        assert_eq!(malformed.outcome, Outcome::DecodeError);
        assert_eq!(
            malformed.ack_reason_code,
            Some(CommandAckReasonCode::ResponseDecodeError)
        );

        assert_eq!(
            captured_envelope_transport_category_matrix().len(),
            transport_categories().len()
        );

        let rendered = serde_json::to_string(&combined).expect("status matrix serializes");
        assert!(!rendered.contains("/v1/accounts/ACC_TEST_0001"));
        assert!(!rendered.contains("ACC_TEST_0001"));
        assert!(!rendered.contains("ORDER_TEST_0003"));
    }

    #[test]
    fn checkpoint_marker_creation_requires_sqlite_commit_proof_not_diagnostic_layer() {
        fn assert_place_signature(
            _f: fn(
                &EndpointGateApproved,
                &GatewayRealOrderEndpointSqliteTransitionCommitProof,
            ) -> Result<
                PlaceEndpointDurableCheckpointApproved,
                GatewayRealOrderEndpointCheckpointMarkerCreationError,
            >,
        ) {
        }
        fn assert_cancel_signature(
            _f: fn(
                &EndpointGateApproved,
                &GatewayRealOrderEndpointSqliteTransitionCommitProof,
            ) -> Result<
                CancelEndpointDurableCheckpointApproved,
                GatewayRealOrderEndpointCheckpointMarkerCreationError,
            >,
        ) {
        }

        assert_place_signature(create_place_checkpoint_marker_after_sqlite_transition);
        assert_cancel_signature(create_cancel_checkpoint_marker_after_sqlite_transition);
        assert_eq!(checkpoint_marker_creation_constructor_count(), 2);

        let valid_place = GatewayRealOrderEndpointSqliteTransitionCommitProof {
            event: OrderPathEvent::BeginSubmit,
            durable_commit_observed: true,
            diagnostic_or_report_source: false,
            request_snapshot_fingerprint: request_fingerprint(
                GatewayRealOrderEndpointOperation::PlaceOrder,
            ),
            marker_single_use: true,
        };
        assert_eq!(
            validate_checkpoint_marker_creation_proof(
                &valid_place,
                OrderPathEvent::BeginSubmit,
                GatewayRealOrderEndpointOperation::PlaceOrder,
            ),
            Ok(())
        );

        let wrong_event = GatewayRealOrderEndpointSqliteTransitionCommitProof {
            event: OrderPathEvent::RequestCancel,
            durable_commit_observed: true,
            diagnostic_or_report_source: false,
            request_snapshot_fingerprint: request_fingerprint(
                GatewayRealOrderEndpointOperation::PlaceOrder,
            ),
            marker_single_use: true,
        };
        assert_eq!(
            validate_checkpoint_marker_creation_proof(
                &wrong_event,
                OrderPathEvent::BeginSubmit,
                GatewayRealOrderEndpointOperation::PlaceOrder,
            ),
            Err(GatewayRealOrderEndpointCheckpointMarkerCreationError::WrongTransitionEvent)
        );

        let uncommitted = GatewayRealOrderEndpointSqliteTransitionCommitProof {
            event: OrderPathEvent::BeginSubmit,
            durable_commit_observed: false,
            diagnostic_or_report_source: false,
            request_snapshot_fingerprint: request_fingerprint(
                GatewayRealOrderEndpointOperation::PlaceOrder,
            ),
            marker_single_use: true,
        };
        assert_eq!(
            validate_checkpoint_marker_creation_proof(
                &uncommitted,
                OrderPathEvent::BeginSubmit,
                GatewayRealOrderEndpointOperation::PlaceOrder,
            ),
            Err(GatewayRealOrderEndpointCheckpointMarkerCreationError::DurableCommitNotObserved)
        );

        let diagnostic_source = GatewayRealOrderEndpointSqliteTransitionCommitProof {
            event: OrderPathEvent::BeginSubmit,
            durable_commit_observed: true,
            diagnostic_or_report_source: true,
            request_snapshot_fingerprint: request_fingerprint(
                GatewayRealOrderEndpointOperation::PlaceOrder,
            ),
            marker_single_use: true,
        };
        assert_eq!(
            validate_checkpoint_marker_creation_proof(
                &diagnostic_source,
                OrderPathEvent::BeginSubmit,
                GatewayRealOrderEndpointOperation::PlaceOrder,
            ),
            Err(GatewayRealOrderEndpointCheckpointMarkerCreationError::DiagnosticOrReportLayer)
        );

        let marker_reuse = GatewayRealOrderEndpointSqliteTransitionCommitProof {
            event: OrderPathEvent::BeginSubmit,
            durable_commit_observed: true,
            diagnostic_or_report_source: false,
            request_snapshot_fingerprint: request_fingerprint(
                GatewayRealOrderEndpointOperation::PlaceOrder,
            ),
            marker_single_use: false,
        };
        assert_eq!(
            validate_checkpoint_marker_creation_proof(
                &marker_reuse,
                OrderPathEvent::BeginSubmit,
                GatewayRealOrderEndpointOperation::PlaceOrder,
            ),
            Err(GatewayRealOrderEndpointCheckpointMarkerCreationError::MarkerAlreadyUsed)
        );

        let wrong_operation = GatewayRealOrderEndpointSqliteTransitionCommitProof {
            event: OrderPathEvent::BeginSubmit,
            durable_commit_observed: true,
            diagnostic_or_report_source: false,
            request_snapshot_fingerprint: request_fingerprint(
                GatewayRealOrderEndpointOperation::CancelOrder,
            ),
            marker_single_use: true,
        };
        assert_eq!(
            validate_checkpoint_marker_creation_proof(
                &wrong_operation,
                OrderPathEvent::BeginSubmit,
                GatewayRealOrderEndpointOperation::PlaceOrder,
            ),
            Err(
                GatewayRealOrderEndpointCheckpointMarkerCreationError::RequestSnapshotFingerprintMismatch
            )
        );

        let raw_identity = GatewayRealOrderEndpointSqliteTransitionCommitProof {
            event: OrderPathEvent::BeginSubmit,
            durable_commit_observed: true,
            diagnostic_or_report_source: false,
            request_snapshot_fingerprint: GatewayRealOrderEndpointRequestSnapshotFingerprint {
                raw_values_exported: true,
                ..request_fingerprint(GatewayRealOrderEndpointOperation::PlaceOrder)
            },
            marker_single_use: true,
        };
        assert_eq!(
            validate_checkpoint_marker_creation_proof(
                &raw_identity,
                OrderPathEvent::BeginSubmit,
                GatewayRealOrderEndpointOperation::PlaceOrder,
            ),
            Err(GatewayRealOrderEndpointCheckpointMarkerCreationError::RawRequestIdentityExported)
        );

        let source = include_str!("real_order_endpoint.rs");
        let marker_source = source
            .split("fn create_place_checkpoint_marker_after_sqlite_transition")
            .nth(1)
            .expect("marker creation source")
            .split("pub fn place_order_api_shape")
            .next()
            .expect("marker creation boundary");
        assert!(marker_source.contains("EndpointGateApproved"));
        assert!(marker_source.contains("GatewayRealOrderEndpointSqliteTransitionCommitProof"));
        assert!(marker_source.contains("validate_request_snapshot_fingerprint"));
        assert!(!marker_source.contains("GatewayRealOrderEndpointFutureSendDiagnostic"));
        assert!(!marker_source.contains("GatewayRealOrderEndpointAcceptedResultDiagnostic"));
        assert!(!marker_source.contains("GatewayRealOrderEndpointApiShape"));
    }
}
