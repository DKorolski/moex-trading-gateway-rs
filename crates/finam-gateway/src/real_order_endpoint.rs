//! Design-only API shape for a future gateway-owned FINAM real order endpoint
//! boundary.
//!
//! This module intentionally does not perform route rendering from live inputs,
//! does not own an HTTP client, and does not send FINAM order requests. It only
//! records the pre-implementation boundary shape that a later reviewed
//! implementation must satisfy.

use serde::{Deserialize, Serialize};

use crate::{
    EndpointGateApproved, M3cOrderEndpointNegativeTestPlanItem,
    M3cOrderEndpointRouteAllowlistEntry, M3cOrderEndpointScannerTransitionMode,
    RuntimeCommandAckIdPolicy,
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
pub struct GatewayRealOrderEndpointRouteShape {
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
    pub runtime_ack_id_policy: RuntimeCommandAckIdPolicy,
    pub place_order_route: GatewayRealOrderEndpointRouteShape,
    pub cancel_order_route: GatewayRealOrderEndpointRouteShape,
    pub scanner_transition_spec: GatewayRealOrderEndpointScannerTransitionSpec,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRealOrderEndpointScannerTransitionSpec {
    pub current_mode: M3cOrderEndpointScannerTransitionMode,
    pub future_mode: M3cOrderEndpointScannerTransitionMode,
    pub exact_place_order_surface_count: usize,
    pub exact_cancel_order_surface_count: usize,
    pub approved_module_path: String,
    pub allowed_routes: Vec<M3cOrderEndpointRouteAllowlistEntry>,
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
        runtime_ack_id_policy: RuntimeCommandAckIdPolicy::RedactedRuntimeAckOnly,
        place_order_route: GatewayRealOrderEndpointRouteShape {
            operation: GatewayRealOrderEndpointOperation::PlaceOrder,
            method_name: "POST".to_string(),
            route_template: "/v1/accounts/{account_id}/orders".to_string(),
            gate_marker_required: true,
        },
        cancel_order_route: GatewayRealOrderEndpointRouteShape {
            operation: GatewayRealOrderEndpointOperation::CancelOrder,
            method_name: "DELETE".to_string(),
            route_template: "/v1/accounts/{account_id}/orders/{order_id}".to_string(),
            gate_marker_required: true,
        },
        scanner_transition_spec: GatewayRealOrderEndpointScannerTransitionSpec {
            current_mode: M3cOrderEndpointScannerTransitionMode::CurrentDenyAllOrderPostDelete,
            future_mode:
                M3cOrderEndpointScannerTransitionMode::FutureExactTwoRouteAllowlistAfterReview,
            exact_place_order_surface_count: 1,
            exact_cancel_order_surface_count: 1,
            approved_module_path,
            allowed_routes: crate::m3c_future_order_endpoint_allowlist(),
            negative_tests: crate::m3c_order_endpoint_negative_test_plan(),
            real_post_delete_calls_allowed_now: false,
        },
    }
}

pub fn place_order_api_shape(
    _gate: &EndpointGateApproved,
    _spec: &broker_finam::FinamPlaceOrderRequestSpec,
) -> GatewayRealOrderEndpointRouteShape {
    api_shape().place_order_route
}

pub fn cancel_order_api_shape(
    _gate: &EndpointGateApproved,
    _spec: &broker_finam::FinamCancelOrderRequestSpec,
) -> GatewayRealOrderEndpointRouteShape {
    api_shape().cancel_order_route
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
        assert_eq!(shape.place_order_route.method_name, "POST");
        assert_eq!(shape.cancel_order_route.method_name, "DELETE");
        assert!(shape.place_order_route.gate_marker_required);
        assert!(shape.cancel_order_route.gate_marker_required);
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
        assert_eq!(shape.scanner_transition_spec.allowed_routes.len(), 2);
        assert_eq!(shape.scanner_transition_spec.negative_tests.len(), 6);
    }

    #[test]
    fn route_shape_functions_require_endpoint_gate_marker_in_signature() {
        fn assert_place_signature(
            _f: fn(
                &EndpointGateApproved,
                &broker_finam::FinamPlaceOrderRequestSpec,
            ) -> GatewayRealOrderEndpointRouteShape,
        ) {
        }
        fn assert_cancel_signature(
            _f: fn(
                &EndpointGateApproved,
                &broker_finam::FinamCancelOrderRequestSpec,
            ) -> GatewayRealOrderEndpointRouteShape,
        ) {
        }

        assert_place_signature(place_order_api_shape);
        assert_cancel_signature(cancel_order_api_shape);
    }
}
