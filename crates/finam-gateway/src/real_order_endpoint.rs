//! Design-only API shape for a future gateway-owned FINAM real order endpoint
//! boundary.
//!
//! This module intentionally does not perform route rendering from live inputs,
//! does not own a network connector, and does not submit FINAM order requests.
//! It only records the pre-implementation boundary shape that a later reviewed
//! implementation must satisfy. Any path template kept here is internal-only;
//! exported diagnostics are redacted.

use serde::{Deserialize, Serialize};

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
pub enum GatewayRealOrderEndpointOperatorPolicy {
    None,
    BackoffAndManualIntervention,
    DegradeAndManualIntervention,
    DisarmAndOperatorIntervention,
    DecodeManualIntervention,
    TransportCategoryManualIntervention,
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
pub struct GatewayRealOrderEndpointApiShape {
    pub mode: GatewayRealOrderEndpointBoundaryMode,
    pub approved_module_path: String,
    pub route_rendering_requires_gate_marker: bool,
    pub http_send_requires_gate_marker: bool,
    pub api_shape_contains_route_templates: bool,
    pub approved_request_parts_design: GatewayRealOrderEndpointApprovedPartsDesignShape,
    pub consumer_design: GatewayRealOrderEndpointConsumerDesignShape,
    pub future_send_result_design: GatewayRealOrderEndpointFutureSendResultDesignShape,
    pub outcome_state_policy_design: GatewayRealOrderEndpointOutcomeStatePolicyDesignShape,
    pub durable_checkpoint_capability_design:
        GatewayRealOrderEndpointDurableCheckpointCapabilityDesignShape,
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
            error_kind: Some(OrderPathErrorKind::TransportTimeout),
            ack_status: CommandAckStatus::Error,
            place_ack_reason_code: Some(CommandAckReasonCode::TransportTimeout),
            cancel_ack_reason_code: Some(CommandAckReasonCode::TransportTimeout),
            operator_policy: OperatorPolicy::TransportCategoryManualIntervention,
            operator_disarm_signal: Some(OperatorDisarmSignal::UnknownPendingOrder),
            backoff_required: false,
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
        assert_eq!(
            transport.error_kind,
            Some(OrderPathErrorKind::TransportTimeout)
        );
        assert_eq!(
            transport.operator_policy,
            OperatorPolicy::TransportCategoryManualIntervention
        );
        assert_eq!(
            transport.operator_disarm_signal,
            Some(OperatorDisarmSignal::UnknownPendingOrder)
        );
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
}
