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

if rg -n '\.post\(|\.delete\(|Method::POST|Method::DELETE' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "design-only API shape must not contain HTTP send surfaces"
fi
rm -f /tmp/moex_transition_forbidden.$$

required_patterns=(
  "EndpointGateApproved"
  "FinamPlaceOrderRequestSpec"
  "FinamCancelOrderRequestSpec"
  "DesignOnlyNoHttpSend"
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
