//! Design-only API shape for a future gateway-owned FINAM real order endpoint
//! boundary.
//!
//! This module intentionally does not perform route rendering from live inputs,
//! does not own a network connector, and does not submit FINAM order requests.
//! It only records the pre-implementation boundary shape that a later reviewed
//! implementation must satisfy.

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointGatedRouteShape {
    pub operation: GatewayRealOrderEndpointOperation,
    pub method_name: String,
    pub route_template: String,
    pub gate_marker_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointApiShape {
    pub mode: GatewayRealOrderEndpointBoundaryMode,
    pub approved_module_path: String,
    pub route_rendering_requires_gate_marker: bool,
    pub http_send_requires_gate_marker: bool,
    pub api_shape_contains_route_templates: bool,
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

fn place_order_route_shape() -> GatewayRealOrderEndpointGatedRouteShape {
    GatewayRealOrderEndpointGatedRouteShape {
        operation: GatewayRealOrderEndpointOperation::PlaceOrder,
        method_name: "POST".to_string(),
        route_template: "/v1/accounts/{account_id}/orders".to_string(),
        gate_marker_required: true,
    }
}

fn cancel_order_route_shape() -> GatewayRealOrderEndpointGatedRouteShape {
    GatewayRealOrderEndpointGatedRouteShape {
        operation: GatewayRealOrderEndpointOperation::CancelOrder,
        method_name: "DELETE".to_string(),
        route_template: "/v1/accounts/{account_id}/orders/{order_id}".to_string(),
        gate_marker_required: true,
    }
}

pub fn place_order_api_shape(
    _gate: &EndpointGateApproved,
    _spec: &broker_finam::FinamPlaceOrderRequestSpec,
) -> GatewayRealOrderEndpointGatedRouteShape {
    place_order_route_shape()
}

pub fn cancel_order_api_shape(
    _gate: &EndpointGateApproved,
    _spec: &broker_finam::FinamCancelOrderRequestSpec,
) -> GatewayRealOrderEndpointGatedRouteShape {
    cancel_order_route_shape()
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
    }

    #[test]
    fn route_shape_functions_require_endpoint_gate_marker_in_signature() {
        fn assert_place_signature(
            _f: fn(
                &EndpointGateApproved,
                &broker_finam::FinamPlaceOrderRequestSpec,
            ) -> GatewayRealOrderEndpointGatedRouteShape,
        ) {
        }
        fn assert_cancel_signature(
            _f: fn(
                &EndpointGateApproved,
                &broker_finam::FinamCancelOrderRequestSpec,
            ) -> GatewayRealOrderEndpointGatedRouteShape,
        ) {
        }

        assert_place_signature(place_order_api_shape);
        assert_cancel_signature(cancel_order_api_shape);
    }

    #[test]
    fn gated_route_shapes_are_separate_from_design_report_shape() {
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
}
