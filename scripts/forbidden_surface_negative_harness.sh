#!/usr/bin/env bash
set -euo pipefail

workspace_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/moex_forbidden_negative.XXXXXX")"
trap 'rm -rf "$tmp_root"' EXIT

cp -R "$workspace_root/scripts" "$tmp_root/scripts"
cp -R "$workspace_root/crates" "$tmp_root/crates"
cp -R "$workspace_root/source-oracles" "$tmp_root/source-oracles"
cp "$workspace_root/Cargo.toml" "$tmp_root/Cargo.toml"
mkdir -p "$tmp_root/config" "$tmp_root/tests/fixtures/stage5"
cp "$workspace_root/config/imoexf-hybrid-high180-profile.redacted.toml" "$tmp_root/config/"
cp "$workspace_root/tests/fixtures/stage5/imoexf_high180_profile_binding.json" "$tmp_root/tests/fixtures/stage5/"
cp "$workspace_root/tests/fixtures/stage5/bracket_terminal_reconciliation.json" "$tmp_root/tests/fixtures/stage5/"

if ! (cd "$tmp_root" && bash scripts/forbidden_surface_scan.sh) >/tmp/moex_negative_scan.$$ 2>&1; then
  cat /tmp/moex_negative_scan.$$ >&2
  rm -f /tmp/moex_negative_scan.$$
  echo "forbidden-surface-negative-harness: copied baseline must pass before negative cases" >&2
  exit 1
fi
rm -f /tmp/moex_negative_scan.$$

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

expect_scanner_failure() {
  local case_name="$1"
  if (cd "$tmp_root" && bash scripts/forbidden_surface_scan.sh) >/tmp/moex_negative_scan.$$ 2>&1; then
    cat /tmp/moex_negative_scan.$$ >&2
    rm -f /tmp/moex_negative_scan.$$
    echo "forbidden-surface-negative-harness: expected failure for $case_name" >&2
    exit 1
  fi
  rm -f /tmp/moex_negative_scan.$$
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
run_negative_case "strategy-integrated-wrapper-oracle-drift" '// stage5 negative wrapper oracle drift' "$tmp_root/source-oracles/alor-stage5/hybrid_intraday_runtime.rs"
run_negative_case "strategy-high180-profile-fixture-drift" '# stage5 negative profile drift' "$tmp_root/config/imoexf-hybrid-high180-profile.redacted.toml"

semantic_ledger="$tmp_root/crates/strategy-runtime-core/source-correspondence.toml"
semantic_target="$tmp_root/crates/strategy-runtime-core/src/hybrid_intraday/intraday_breakout.rs"
semantic_ledger_backup="$semantic_ledger.bak"
semantic_target_backup="$semantic_target.bak"
cp "$semantic_ledger" "$semantic_ledger_backup"
cp "$semantic_target" "$semantic_target_backup"
perl -0pi -e 's/k: 0\.65,/k: 0.66,/' "$semantic_target"
changed_target_sha="$(shasum -a 256 "$semantic_target" | awk '{print $1}')"
perl -0pi -e 's/target_sha256 = "a3b125f282f201b66dfa8d2685f22aa94048856a5145d537b76dc8934a5f9ae5"/target_sha256 = "'"$changed_target_sha"'"/' "$semantic_ledger"
if (cd "$tmp_root" && bash scripts/forbidden_surface_scan.sh) >/tmp/moex_negative_scan.$$ 2>&1; then
  cat /tmp/moex_negative_scan.$$ >&2
  rm -f /tmp/moex_negative_scan.$$
  echo "forbidden-surface-negative-harness: expected immutable manifest failure for formula change plus ledger rehash" >&2
  exit 1
fi
rm -f /tmp/moex_negative_scan.$$
mv "$semantic_ledger_backup" "$semantic_ledger"
mv "$semantic_target_backup" "$semantic_target"

semantic_ledger_backup="$semantic_ledger.bak"
cp "$semantic_ledger" "$semantic_ledger_backup"
perl -0pi -e 's/alor_source_commit = "43242c89944d335d9cb0729b38bdd7d658378d5e"/alor_source_commit = "deadbeef"/' "$semantic_ledger"
if (cd "$tmp_root" && bash scripts/forbidden_surface_scan.sh) >/tmp/moex_negative_scan.$$ 2>&1; then
  cat /tmp/moex_negative_scan.$$ >&2
  rm -f /tmp/moex_negative_scan.$$
  echo "forbidden-surface-negative-harness: expected immutable source commit failure" >&2
  exit 1
fi
rm -f /tmp/moex_negative_scan.$$
mv "$semantic_ledger_backup" "$semantic_ledger"

root_manifest="$tmp_root/Cargo.toml"
root_manifest_backup="$root_manifest.bak"
cp "$root_manifest" "$root_manifest_backup"
perl -0pi -e 's/\s*"crates\/strategy-runtime-core",\n//' "$root_manifest"
expect_scanner_failure "remove-strategy-runtime-core-from-workspace"
mv "$root_manifest_backup" "$root_manifest"

semantic_manifest="$tmp_root/crates/strategy-runtime-core/Cargo.toml"
semantic_manifest_backup="$semantic_manifest.bak"
semantic_alternate="$tmp_root/crates/strategy-runtime-core/src/alternate.rs"
cp "$semantic_manifest" "$semantic_manifest_backup"
printf '\npub fn alternate_semantic_root() {}\n' > "$semantic_alternate"
printf '\n[lib]\npath = "src/alternate.rs"\n' >> "$semantic_manifest"
expect_scanner_failure "redirect-strategy-runtime-core-lib-path"
rm -f "$semantic_alternate"
mv "$semantic_manifest_backup" "$semantic_manifest"

semantic_lib="$tmp_root/crates/strategy-runtime-core/src/lib.rs"
semantic_lib_backup="$semantic_lib.bak"
semantic_wrapper="$tmp_root/crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs"
cp "$semantic_lib" "$semantic_lib_backup"
printf '\npub fn untracked_wrapper() {}\n' > "$semantic_wrapper"
printf '\npub mod hybrid_intraday_runtime;\n' >> "$semantic_lib"
expect_scanner_failure "add-untracked-stage5b2-wrapper-and-export"
rm -f "$semantic_wrapper"
mv "$semantic_lib_backup" "$semantic_lib"

semantic_manifest_backup="$semantic_manifest.bak"
cp "$semantic_manifest" "$semantic_manifest_backup"
perl -0pi -e 's/\[package\]/[package]\nautotests = false/' "$semantic_manifest"
expect_scanner_failure "disable-strategy-runtime-core-tests"
mv "$semantic_manifest_backup" "$semantic_manifest"

bracket_fixture="$tmp_root/tests/fixtures/stage5/bracket_terminal_reconciliation.json"
bracket_fixture_backup="$bracket_fixture.bak"
cp "$bracket_fixture" "$bracket_fixture_backup"
perl -0pi -e 's/"grace_ms": 3000/"grace_ms": 4000/' "$bracket_fixture"
expect_scanner_failure "drift-bracket-terminal-reconciliation-fixture"
mv "$bracket_fixture_backup" "$bracket_fixture"

echo "forbidden-surface-negative-harness: ok"
