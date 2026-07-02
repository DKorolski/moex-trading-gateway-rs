#!/usr/bin/env bash
set -euo pipefail

workspace_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$workspace_root"

target="crates/finam-gateway/src/real_order_endpoint.rs"

if [[ ! -f "$target" ]]; then
  echo "order-endpoint-scanner-transition-spec: missing $target" >&2
  exit 1
fi

failures=0

report_failure() {
  echo "order-endpoint-scanner-transition-spec: $*" >&2
  failures=$((failures + 1))
}

if rg -n '\.post\(|\.delete\(|\.request\(|\.send\(|Method::POST|Method::DELETE|reqwest|HttpClient|\bTransport\b|\bAdapter\b|\bBackend\b' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "design-only API shape must not contain HTTP send surfaces"
fi
rm -f /tmp/moex_transition_forbidden.$$

if rg -n 'pub struct GatewayRealOrderEndpointInternalRouteShape' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "internal route shape must not be public"
fi
rm -f /tmp/moex_transition_forbidden.$$

if rg -n 'pub fn consume_approved_request_parts_for_future_endpoint' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "approved request-parts consumer must not be public"
fi
rm -f /tmp/moex_transition_forbidden.$$

if rg -n 'pub fn classify_future_send_attempt_result' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "future send result classifier must not be public"
fi
rm -f /tmp/moex_transition_forbidden.$$

if rg -n 'consume_approved_request_parts_for_future_endpoint\([^)]*GatewayRealOrderEndpoint.*Diagnostic' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "diagnostic DTOs must not feed the approved request-parts consumer"
fi
rm -f /tmp/moex_transition_forbidden.$$

if rg -n 'classify_future_send_attempt_result\([^)]*GatewayRealOrderEndpoint.*Diagnostic' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "diagnostic DTOs must not feed the future send result classifier"
fi
rm -f /tmp/moex_transition_forbidden.$$

for internal_type in \
  GatewayRealOrderEndpointInternalRouteShape \
  RenderedOrderEndpointPath \
  ApprovedOrderEndpointRequestParts
do
  if rg -n "pub struct ${internal_type}" "$target" >/tmp/moex_transition_forbidden.$$; then
    cat /tmp/moex_transition_forbidden.$$ >&2
    report_failure "${internal_type} must not be public"
  fi
  rm -f /tmp/moex_transition_forbidden.$$

  if rg -n "impl std::fmt::Debug for ${internal_type}" "$target" >/tmp/moex_transition_forbidden.$$; then
    cat /tmp/moex_transition_forbidden.$$ >&2
    report_failure "${internal_type} must not implement Debug"
  fi
  rm -f /tmp/moex_transition_forbidden.$$

  if rg -nU "#\\[derive\\([^\\]]*(Debug|Serialize|Deserialize)[^\\]]*\\)\\]\\nstruct ${internal_type}" "$target" >/tmp/moex_transition_forbidden.$$; then
    cat /tmp/moex_transition_forbidden.$$ >&2
    report_failure "${internal_type} must not be Debug/Serialize/Deserialize"
  fi
  rm -f /tmp/moex_transition_forbidden.$$
done

if rg -nU '#\[derive\([^\]]*Serialize[^\]]*\)\]\nstruct GatewayRealOrderEndpointInternalRouteShape' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "internal route shape must not be serializable"
fi
rm -f /tmp/moex_transition_forbidden.$$

required_patterns=(
  "EndpointGateApproved"
  "FinamPlaceOrderRequestSpec"
  "FinamCancelOrderRequestSpec"
  "DesignOnlyNoHttpSend"
  "api_shape_contains_route_templates: false"
  "struct GatewayRealOrderEndpointInternalRouteShape"
  "struct RenderedOrderEndpointPath"
  "struct ApprovedOrderEndpointRequestParts"
  "fn consume_approved_request_parts_for_future_endpoint"
  "diagnostic_can_construct_request_parts: false"
  "constructors_require_endpoint_gate: true"
  "constructors_require_approved_request_spec: true"
  "constructors_require_account_instrument_allowlist: true"
  "constructors_require_operator_arm: true"
  "constructors_require_durable_state_checkpoint: true"
  "constructors_require_operation_specific_checkpoint: true"
  "consumer_internal_only: true"
  "consumer_requires_endpoint_gate: true"
  "consumer_accepts_approved_request_parts_only: true"
  "consumer_accepts_diagnostics: false"
  "consumer_network_enabled: false"
  "GatewayRealOrderEndpointFutureSendOutcome"
  "Accepted"
  "Rejected"
  "TimeoutUnknownPending"
  "RateLimited"
  "Maintenance"
  "Unauthorized"
  "DecodeError"
  "TransportError"
  "fn classify_future_send_attempt_result"
  "future_send_requires_endpoint_gate: true"
  "future_send_accepts_approved_request_parts_only: true"
  "future_send_accepts_diagnostics: false"
  "future_send_consumes_request_parts: true"
  "future_send_network_enabled: false"
  "operation_specific_durable_checkpoint_required: true"
  "retry_after_timeout_unknown_allowed: false"
  "request_parts_reuse_after_outcome_allowed: false"
  "result_diagnostic_can_bypass_state_machine: false"
  "state_machine_transition_required: true"
  "PlaceBeginSubmitPersistedBeforeEndpoint"
  "CancelRequestCancelPersistedBeforeEndpoint"
  "route_template_exported: false"
  "rendered_path_exported: false"
  "raw_body_exported: false"
  "GatewayRealOrderEndpointRedactedRouteDiagnostic"
  "GatewayRealOrderEndpointApprovedPartsDiagnostic"
  "GatewayRealOrderEndpointConsumerDiagnostic"
  "GatewayRealOrderEndpointFutureSendDiagnostic"
  "CurrentDenyAllOrderPostDelete"
  "FutureExactTwoRouteAllowlistAfterReview"
  "real_post_delete_calls_allowed_now: false"
)

for pattern in "${required_patterns[@]}"; do
  if ! rg -n "$pattern" "$target" >/dev/null; then
    report_failure "required API-shape marker missing: $pattern"
  fi
done

if (( failures > 0 )); then
  exit 1
fi

echo "order-endpoint-scanner-transition-spec: ok"
