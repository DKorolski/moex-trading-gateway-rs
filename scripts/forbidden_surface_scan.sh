#!/usr/bin/env bash
set -euo pipefail

workspace_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$workspace_root"

failures=0

report_failure() {
  echo "forbidden-surface-scan: $*" >&2
  failures=$((failures + 1))
}

approved_order_transport="crates/finam-gateway/src/m3d2_real_order_transport.rs"

if rg -n --glob 'crates/**/*.rs' '\.delete\(' crates | grep -v "^${approved_order_transport}:" >/tmp/moex_forbidden_delete.$$; then
  cat /tmp/moex_forbidden_delete.$$ >&2
  report_failure "real HTTP DELETE surface is forbidden outside the reviewed M3d-2c transport"
fi
rm -f /tmp/moex_forbidden_delete.$$

if rg -n --glob 'crates/**/*.rs' 'Method::DELETE' crates >/tmp/moex_forbidden_method_delete.$$; then
  cat /tmp/moex_forbidden_method_delete.$$ >&2
  report_failure "Method::DELETE surface is forbidden"
fi
rm -f /tmp/moex_forbidden_method_delete.$$

if rg -n --glob 'crates/**/*.rs' 'Method::POST' crates >/tmp/moex_forbidden_method_post.$$; then
  cat /tmp/moex_forbidden_method_post.$$ >&2
  report_failure "Method::POST is not allowed in gateway/order surfaces"
fi
rm -f /tmp/moex_forbidden_method_post.$$

if rg -n '"/v1/accounts/[^"]*/orders' crates/broker-finam/src/lib.rs >/tmp/moex_forbidden_order_route_literal.$$; then
  cat /tmp/moex_forbidden_order_route_literal.$$ >&2
  report_failure "literal FINAM order route bypass is forbidden before explicit endpoint review"
fi
rm -f /tmp/moex_forbidden_order_route_literal.$$

if rg -n --glob 'crates/**/*.rs' 'OrderEndpointHttp(Client|Transport|Adapter|Backend)' crates >/tmp/moex_forbidden_order_http_abstraction.$$; then
  cat /tmp/moex_forbidden_order_http_abstraction.$$ >&2
  report_failure "non-reqwest order endpoint HTTP abstraction is forbidden before explicit endpoint review"
fi
rm -f /tmp/moex_forbidden_order_http_abstraction.$$

if rg -n --glob 'crates/**/*.rs' 'EndpointGateApproved[[:space:]]*\{[[:space:]]*_private:[[:space:]]*\(\)' crates >/tmp/moex_forbidden_endpoint_gate_literal.$$; then
  cat /tmp/moex_forbidden_endpoint_gate_literal.$$ >&2
  report_failure "direct EndpointGateApproved literal construction is forbidden outside reviewed constructors"
fi
rm -f /tmp/moex_forbidden_endpoint_gate_literal.$$

python3 - <<'PY'
from pathlib import Path
import sys

failures = 0

for path in Path("crates").glob("**/*.rs"):
    source = path.read_text()
    if ".post(" not in source:
        continue
    if path == Path("crates/finam-gateway/src/m3d2_real_order_transport.rs"):
        if source.count(".post(") != 1:
            print(
                "forbidden-surface-scan: M3d-2c transport must have exactly one .post(",
                file=sys.stderr,
            )
            failures += 1
        continue
    if path != Path("crates/broker-finam/src/lib.rs"):
        for line_no, line in enumerate(source.splitlines(), start=1):
            if ".post(" in line:
                print(
                    f"forbidden-surface-scan: unexpected .post( in {path}:{line_no}",
                    file=sys.stderr,
                )
                failures += 1
        continue

    allowed_functions = {
        "auth": 'self.rest_url(&["v1", "sessions"])',
        "token_details": 'self.rest_url(&["v1", "sessions", "details"])',
        "token_details_typed": 'self.rest_url(&["v1", "sessions", "details"])',
    }
    for function_name, expected_path in allowed_functions.items():
        marker = f"pub async fn {function_name}("
        if marker not in source:
            print(
                f"forbidden-surface-scan: cannot locate allowed POST function {function_name}",
                file=sys.stderr,
            )
            failures += 1
            continue
        block = source.split(marker, 1)[1]
        next_function = block.find("\n    pub async fn ")
        if next_function != -1:
            block = block[:next_function]
        post_count = block.count(".post(")
        if post_count != 1 or expected_path not in block:
            print(
                "forbidden-surface-scan: broker-finam POST allowlist mismatch "
                f"for {function_name}: post_count={post_count}",
                file=sys.stderr,
            )
            failures += 1
    allowed_post_count = len(allowed_functions)
    actual_post_count = source.count(".post(")
    if actual_post_count != allowed_post_count:
        print(
            "forbidden-surface-scan: broker-finam has unexpected .post( count "
            f"actual={actual_post_count} allowed={allowed_post_count}",
            file=sys.stderr,
        )
        failures += 1

transport_path = Path("crates/finam-gateway/src/m3d2_real_order_transport.rs")
if transport_path.exists():
    transport_source = transport_path.read_text()
    expected_counts = {
        ".post(": 1,
        ".delete(": 1,
        ".send(": 1,
    }
    for token, expected in expected_counts.items():
        actual = transport_source.count(token)
        if actual != expected:
            print(
                "forbidden-surface-scan: M3d-2c transport allowlist mismatch "
                f"for {token}: actual={actual} expected={expected}",
                file=sys.stderr,
            )
            failures += 1
    required_transport_patterns = [
        "EndpointGateApproved",
        "FinamPlaceOrderRequestSpec",
        "FinamCancelOrderRequestSpec",
        "FinamAuthorizationHeaderMode::BearerJwt",
        "post_send_semantics",
        "raw_token_exported: false",
        "raw_path_exported: false",
        "raw_body_exported: false",
    ]
    for pattern in required_transport_patterns:
        if pattern not in transport_source:
            print(
                "forbidden-surface-scan: M3d-2c transport missing required "
                f"pattern {pattern!r}",
                file=sys.stderr,
            )
            failures += 1

source = Path("crates/finam-gateway/src/lib.rs").read_text()

scopes = {
    "real-readonly transport": (
        "pub struct ReqwestFinamRealReadonlyBrokerTruthTransport",
        "#[derive(Clone)]\npub struct LocalMockFinamRealReadonlyBrokerTruthTransport",
    ),
    "real-readonly operator probe": (
        "pub struct FinamRealReadonlyContractProbeOperatorRunConfig",
        "#[derive(Clone)]\npub struct LocalMockCancelBrokerTruthReadonlyHttpClient",
    ),
}

forbidden = [
    ".post(",
    ".delete(",
    "Method::POST",
    "Method::DELETE",
    "FinamPlaceOrderRequestSpec",
    "FinamCancelOrderRequestSpec",
    "FinamRealOrderEndpointTransport",
    "place_order_endpoint",
    "cancel_order_endpoint",
]

for scope_name, (start, end) in scopes.items():
    try:
        scoped = source.split(start, 1)[1].split(end, 1)[0]
    except IndexError:
        print(f"forbidden-surface-scan: cannot locate {scope_name} scope", file=sys.stderr)
        failures += 1
        continue
    for pattern in forbidden:
        if pattern in scoped:
            print(
                f"forbidden-surface-scan: {pattern!r} found in {scope_name}",
                file=sys.stderr,
            )
            failures += 1

if "\npub async fn run_finam_real_readonly_contract_probe" in source:
    print(
        "forbidden-surface-scan: lower-level real-readonly contract probe must not be public",
        file=sys.stderr,
    )
    failures += 1
if "\npub async fn run_finam_real_readonly_operator_contract_probe" not in source:
    print(
        "forbidden-surface-scan: operator real-readonly contract probe entrypoint missing",
        file=sys.stderr,
    )
    failures += 1

sys.exit(1 if failures else 0)
PY

if (( failures > 0 )); then
  exit 1
fi

echo "forbidden-surface-scan: ok"
