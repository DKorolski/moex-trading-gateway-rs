#!/usr/bin/env bash
set -euo pipefail

workspace_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/moex_forbidden_negative.XXXXXX")"
trap 'rm -rf "$tmp_root"' EXIT

cp -R "$workspace_root/scripts" "$tmp_root/scripts"
cp -R "$workspace_root/crates" "$tmp_root/crates"

run_negative_case() {
  local case_name="$1"
  local injection="$2"
  local target="${3:-$tmp_root/crates/broker-finam/src/lib.rs}"
  local backup="$tmp_root/crates/broker-finam/src/lib.rs.bak"

  backup="$target.bak"

  cp "$target" "$backup"
  printf '\n%s\n' "$injection" >> "$target"
  if (cd "$tmp_root" && bash scripts/forbidden_surface_scan.sh) >/tmp/moex_negative_scan.$$ 2>&1; then
    cat /tmp/moex_negative_scan.$$ >&2
    rm -f /tmp/moex_negative_scan.$$
    echo "forbidden-surface-negative-harness: expected failure for $case_name" >&2
    exit 1
  fi
  rm -f /tmp/moex_negative_scan.$$
  mv "$backup" "$target"
}

run_negative_case "same-module-extra-post" 'fn _m3c_negative_same_module_post(client: reqwest::Client, url: &str) { let _ = client.post(url); }'
run_negative_case "same-module-extra-delete" 'fn _m3c_negative_same_module_delete(client: reqwest::Client, url: &str) { let _ = client.delete(url); }'
run_negative_case "generic-method-post" 'fn _m3c_negative_generic_post() { let _ = reqwest::Method::POST; }'
run_negative_case "generic-method-delete" 'fn _m3c_negative_generic_delete() { let _ = reqwest::Method::DELETE; }'
run_negative_case "route-string-bypass" 'fn _m3c_negative_route_bypass() -> String { "/v1/accounts/ACC_TEST_0001/orders".to_string() }'
run_negative_case "non-reqwest-client-abstraction" 'trait OrderEndpointHttpClient { fn send_order_endpoint(&self, route: &str); }'
run_negative_case "wrong-module-post-delete" 'fn _m3d_negative_wrong_module(client: reqwest::Client, url: &str) { let _ = client.post(url); let _ = client.delete(url); }' "$tmp_root/crates/finam-gateway/src/lib.rs"
run_negative_case "sltp-bracket-endpoint-expansion" 'fn _m3d_negative_sltp_bracket(client: reqwest::Client, url: &str) { let _ = client.post(url); }'
run_negative_case "runtime-command-consumer-bypass" 'fn _m3d_negative_runtime_bypass() { let _ = reqwest::Method::POST; }' "$tmp_root/crates/finam-gateway/src/lib.rs"
run_negative_case "strategy-semantic-kernel-transport-dependency" 'fn _stage5_negative_transport() { let _ = reqwest::Method::POST; }' "$tmp_root/crates/strategy-runtime-core/src/lib.rs"
run_negative_case "strategy-semantic-source-correspondence-drift" '// stage5 negative source drift' "$tmp_root/crates/strategy-runtime-core/src/hybrid_intraday/types.rs"

echo "forbidden-surface-negative-harness: ok"
