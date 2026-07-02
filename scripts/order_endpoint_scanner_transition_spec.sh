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

if rg -n '\.post\(|\.delete\(|\.request\(|\.send\(|Method::POST|Method::DELETE|reqwest|HttpClient|Transport|Adapter|Backend' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "design-only API shape must not contain HTTP send surfaces"
fi
rm -f /tmp/moex_transition_forbidden.$$

if rg -n 'pub struct GatewayRealOrderEndpointInternalRouteShape' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "internal route shape must not be public"
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
  "diagnostic_can_construct_request_parts: false"
  "constructors_require_endpoint_gate: true"
  "constructors_require_approved_request_spec: true"
  "constructors_require_account_instrument_allowlist: true"
  "constructors_require_operator_arm: true"
  "constructors_require_durable_state_checkpoint: true"
  "route_template_exported: false"
  "rendered_path_exported: false"
  "raw_body_exported: false"
  "GatewayRealOrderEndpointRedactedRouteDiagnostic"
  "GatewayRealOrderEndpointApprovedPartsDiagnostic"
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
