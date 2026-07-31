#!/usr/bin/env bash
set -euo pipefail

# Execute from an immutable temporary copy so self-protection mutations can alter
# the repository copy without changing the script currently being interpreted.
if [[ "${FORBIDDEN_NEGATIVE_WORKER_SNAPSHOT:-0}" != "1" ]]; then
  worker_snapshot="$(mktemp "${TMPDIR:-/tmp}/forbidden-worker.XXXXXX")"
  cp "$0" "$worker_snapshot"
  chmod +x "$worker_snapshot"
  set +e
  FORBIDDEN_NEGATIVE_WORKER_SNAPSHOT=1 bash "$worker_snapshot" "$@"
  worker_status=$?
  set -e
  rm -f "$worker_snapshot"
  exit "$worker_status"
fi

implemented_cases=(
  'same-module-extra-post|failure'
  'same-module-extra-delete|failure'
  'generic-method-post|failure'
  'generic-method-delete|failure'
  'route-string-bypass|failure'
  'non-reqwest-client-abstraction|failure'
  'wrong-module-post-delete|failure'
  'sltp-bracket-endpoint-expansion|failure'
  'runtime-command-consumer-bypass|failure'
  'strategy-semantic-kernel-transport-dependency|failure'
  'strategy-semantic-source-correspondence-drift|failure'
  'riskgate-source-codec-drift|failure'
  'riskgate-stage5d-consumer-drift|failure'
  'riskgate-authority-writer-wrapper-replacement|failure'
  'controlled-source-extension-region-removed|failure'
  'controlled-source-self-authorized-baseline-update|failure'
  'immutable-stage5c-manifest-rewrite|failure'
  'strategy-integrated-wrapper-oracle-drift|failure'
  'strategy-high180-profile-fixture-drift|failure'
  'stage5c-paper-host-source-drift|failure'
  'stage5c-paper-host-fixture-drift|failure'
  'stage5cb-bootstrap-fixture-drift|failure'
  'stage5cc-restore-fixture-drift|failure'
  'stage5cd-warmup-fixture-drift|failure'
  'stage5ce-recovery-fixture-drift|failure'
  'stage5cf-semantic-fixture-drift|failure'
  'stage5cg-settlement-fixture-drift|failure'
  'stage5ch-next-bar-loop-fixture-drift|failure'
  'stage5ci-paper-intent-lifecycle-fixture-drift|failure'
  'stage5cj-paper-broker-lifecycle-fixture-drift|failure'
  'stage5c-api-freeze-manifest-drift|failure'
  'stage5c-empty-evidence-map-valid-json|failure'
  'stage5c-remove-evidence-transition-valid-json|failure'
  'stage5c-remove-source-hash-path-valid-json|failure'
  'stage5c-alter-baseline-full-commit-valid-json|failure'
  'stage5c-alter-baseline-handoff-sha-valid-json|failure'
  'stage5c-remove-accepted-slice-valid-json|failure'
  'stage5c-remove-public-type-valid-json|failure'
  'stage5c-remove-public-method-valid-json|failure'
  'semantic-formula-drift-with-ledger-rehash|failure'
  'semantic-source-commit-drift|failure'
  'remove-strategy-runtime-core-from-workspace|failure'
  'comment-out-workspace-member-but-leave-quoted-comment|failure'
  'redirect-strategy-runtime-core-lib-path|failure'
  'add-alternate-stage5b2-wrapper-and-export|failure'
  'disable-strategy-runtime-core-tests|failure'
  'add-default-build-script|failure'
  'add-untracked-integration-wrapper-target|failure'
  'add-untracked-bench-wrapper-target|failure'
  'add-untracked-example-wrapper-target|failure'
  'copy-wrapper-to-another-workspace-crate-and-export|failure'
  'add-wrapper-in-new-workspace-member-outside-crates|failure'
  'workspace-exclude-drift|failure'
  'unapproved-path-dependency-edge|failure'
  'excluded-local-path-dependency-wrapper|failure'
  'workspace-member-build-rs|failure'
  'repository-local-cargo-config|failure'
  'explicit-target-escapes-declaring-member|failure'
  'duplicate-oracle-under-alias-filename|failure'
  'macro-meta-path-to-renamed-wrapper-inc|failure'
  'comment-separated-wrapper-definition|failure'
  'macro-generated-wrapper-definition|failure'
  'include-wrapper-oracle|failure'
  'split-path-include-wrapper-oracle|failure'
  'any-include-macro-before-wrapper-gate|failure'
  'comment-separated-include-macro|failure'
  'nested-comment-separated-include-macro|failure'
  'raw-identifier-include-macro|failure'
  'macro-indirected-include-activation|failure'
  'path-attribute-wrapper-oracle|failure'
  'comment-separated-path-attribute|failure'
  'cfg-attr-path-wrapper-activation|failure'
  'macro-meta-path-wrapper-activation|failure'
  'split-path-oracle-include-str-outside-allowlist|failure'
  'escaped-oracle-include-str-outside-allowlist|failure'
  'unicode-escaped-oracle-filename|failure'
  'stringify-split-oracle-include-str|failure'
  'drift-bracket-terminal-reconciliation-fixture|failure'
  'close-stage5b2-wrapper-compiled-milestone|failure'
  'open-stage5b2-runtime-host-boundary|failure'
  'forbidden-harness-marker-check-removal|failure'
  'forbidden-harness-inventory-reduction|failure'
  'forbidden-worker-contract-drift|failure'
  'forbidden-ci-timeout-lowered|failure'
  'forbidden-baseline-positive-bypass|failure'
  'forbidden-scanner-contract-drift|failure'
  'include-and-path-text-outside-rust-code|success'
)

if [[ "${1:-}" == "--list-cases" ]]; then
  printf '%s\n' "${implemented_cases[@]}"
  exit 0
fi
if [[ "${1:-}" != "--run-case" || "$#" -ne 4 ]]; then
  echo "usage: $0 --list-cases | --run-case <name> <marker> <failure|success>" >&2
  exit 2
fi

case_filter="$2"
expected_marker="$3"
expected_kind="$4"
tmp_root="$(pwd)"
scan_log="$(mktemp "${TMPDIR:-/tmp}/forbidden-case.XXXXXX")"
case_executed=0
trap 'rm -f "$scan_log"' EXIT

should_run_case() {
  [[ "$case_filter" == "$1" ]]
}

check_scanner_result() {
  local case_name="$1"
  local exit_code="$2"
  case_executed=$((case_executed + 1))
  if [[ "$expected_kind" == "failure" && "$exit_code" -eq 0 ]]; then
    cat "$scan_log" >&2
    echo "forbidden case unexpectedly passed: $case_name" >&2
    exit 1
  fi
  if [[ "$expected_kind" == "success" && "$exit_code" -ne 0 ]]; then
    cat "$scan_log" >&2
    echo "positive control unexpectedly failed: $case_name" >&2
    exit 1
  fi
  if rg -n 'Traceback|No such file or directory|command not found|Python 3\.11\+ with stdlib tomllib is required|worker timed out' "$scan_log" >/dev/null; then
    cat "$scan_log" >&2
    echo "infrastructure failure is not valid case evidence: $case_name" >&2
    exit 1
  fi
  if [[ "$expected_marker" == "__DISCOVER__" ]]; then
    echo "DISCOVER $case_name" >&2
    cat "$scan_log" >&2
    exit 1
  fi
  if ! grep -F -- "$expected_marker" "$scan_log" >/dev/null; then
    cat "$scan_log" >&2
    echo "expected marker missing for $case_name: $expected_marker" >&2
    exit 1
  fi
}

scan_selected_case() {
  local case_name="$1"
  local exit_code=0
  if ! (cd "$tmp_root" && bash scripts/forbidden_surface_scan.sh) >"$scan_log" 2>&1; then
    exit_code=$?
    # `!` normalizes `$?`; use the observed non-zero contract explicitly.
    exit_code=1
  fi
  check_scanner_result "$case_name" "$exit_code"
}

run_negative_case() {
  local case_name="$1"
  local injection="$2"
  if ! should_run_case "$case_name"; then
    return 0
  fi
  local target="${3:-$tmp_root/crates/broker-finam/src/lib.rs}"
  local backup="$tmp_root/crates/broker-finam/src/lib.rs.bak"

  backup="$target.bak"

  cp "$target" "$backup"
  printf '\n%s\n' "$injection" >> "$target"
  scan_selected_case "$case_name"
  mv "$backup" "$target"
}

run_replacement_case() {
  local case_name="$1"
  local target="$2"
  local old="$3"
  local new="$4"
  if ! should_run_case "$case_name"; then
    return 0
  fi
  local backup="$target.bak"
  cp "$target" "$backup"
  python3 - "$target" "$old" "$new" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
old = sys.argv[2]
new = sys.argv[3]
source = path.read_text()
if source.count(old) < 1:
    raise SystemExit(f"replacement contract mismatch for {path}: count={source.count(old)}")
path.write_text(source.replace(old, new, 1))
PY
  scan_selected_case "$case_name"
  mv "$backup" "$target"
}

expect_scanner_failure() {
  local case_name="$1"
  if ! should_run_case "$case_name"; then
    return 0
  fi
  scan_selected_case "$case_name"
}

expect_scanner_success() {
  local case_name="$1"
  if ! should_run_case "$case_name"; then
    return 0
  fi
  scan_selected_case "$case_name"
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
run_negative_case "riskgate-source-codec-drift" '// stage5d negative riskgate source codec drift' "$tmp_root/crates/strategy-runtime-core/src/hybrid_intraday/risk_gate.rs"
run_negative_case "riskgate-stage5d-consumer-drift" '// stage5d negative riskgate consumer drift' "$tmp_root/crates/strategy-runtime-core/src/stage5d_persistence.rs"

run_replacement_case \
  "riskgate-authority-writer-wrapper-replacement" \
  "$tmp_root/crates/strategy-runtime-core/src/hybrid_intraday/risk_gate.rs" \
  "pub(crate) fn authority_redis_fields" \
  "pub(crate) fn legacy_authority_redis_fields"

stage5d_manifest="$tmp_root/docs/stage-5/stage-5d-additive-freeze-manifest.json"
stage5d_manifest_backup="$stage5d_manifest.bak"
run_stage5d_manifest_mutation_case() {
  local case_name="$1"
  local mutation="$2"

  cp "$stage5d_manifest" "$stage5d_manifest_backup"
  MUTATION="$mutation" STAGE5D_MANIFEST="$stage5d_manifest" python3 - <<'PY'
import json
import os
from pathlib import Path

path = Path(os.environ["STAGE5D_MANIFEST"])
mutation = os.environ["MUTATION"]
manifest = json.loads(path.read_text())

if mutation == "remove_controlled_extension":
    manifest["controlled_source_semantic_extensions"] = manifest["controlled_source_semantic_extensions"][1:]
elif mutation == "self_authorized_baseline":
    manifest["controlled_source_semantic_extensions"][0]["stage5c_baseline_sha256"] = manifest["controlled_source_semantic_extensions"][0]["current_sha256"]
elif mutation == "rewrite_stage5c_closure_sha":
    manifest["stage5c_closure_baseline"]["manifest_sha256"] = "0" * 64
else:
    raise SystemExit(f"unknown mutation: {mutation}")

path.write_text(json.dumps(manifest, indent=2) + "\n")
PY
  expect_scanner_failure "$case_name"
  mv "$stage5d_manifest_backup" "$stage5d_manifest"
}

run_stage5d_manifest_mutation_case "controlled-source-extension-region-removed" "remove_controlled_extension"
run_stage5d_manifest_mutation_case "controlled-source-self-authorized-baseline-update" "self_authorized_baseline"
run_stage5d_manifest_mutation_case "immutable-stage5c-manifest-rewrite" "rewrite_stage5c_closure_sha"
run_negative_case "strategy-integrated-wrapper-oracle-drift" '// stage5 negative wrapper oracle drift' "$tmp_root/source-oracles/alor-stage5/hybrid_intraday_runtime.rs"
run_negative_case "strategy-high180-profile-fixture-drift" '# stage5 negative profile drift' "$tmp_root/config/imoexf-hybrid-high180-profile.redacted.toml"
run_negative_case "stage5c-paper-host-source-drift" '// stage5c negative admission drift' "$tmp_root/crates/strategy-runtime-core/src/stage5c_paper_host.rs"
run_negative_case "stage5c-paper-host-fixture-drift" '# stage5c negative fixture drift' "$tmp_root/tests/fixtures/stage5/stage5c_paper_host_admission.json"
run_negative_case "stage5cb-bootstrap-fixture-drift" '# stage5cb negative fixture drift' "$tmp_root/tests/fixtures/stage5/stage5cb_bootstrap_notification.json"
run_negative_case "stage5cc-restore-fixture-drift" '# stage5cc negative fixture drift' "$tmp_root/tests/fixtures/stage5/stage5cc_runtime_state_restore.json"
run_negative_case "stage5cd-warmup-fixture-drift" '# stage5cd negative fixture drift' "$tmp_root/tests/fixtures/stage5/stage5cd_history_warmup.json"
run_negative_case "stage5ce-recovery-fixture-drift" '# stage5ce negative fixture drift' "$tmp_root/tests/fixtures/stage5/stage5ce_pending_recovery.json"
run_negative_case "stage5cf-semantic-fixture-drift" '# stage5cf negative fixture drift' "$tmp_root/tests/fixtures/stage5/stage5cf_semantic_bar.json"
run_negative_case "stage5cg-settlement-fixture-drift" '# stage5cg negative fixture drift' "$tmp_root/tests/fixtures/stage5/stage5cg_paper_intent_settlement.json"
run_negative_case "stage5ch-next-bar-loop-fixture-drift" '# stage5ch negative fixture drift' "$tmp_root/tests/fixtures/stage5/stage5ch_controlled_next_bar_loop.json"
run_negative_case "stage5ci-paper-intent-lifecycle-fixture-drift" '# stage5ci negative fixture drift' "$tmp_root/tests/fixtures/stage5/stage5ci_paper_intent_lifecycle.json"
run_negative_case "stage5cj-paper-broker-lifecycle-fixture-drift" '# stage5cj negative fixture drift' "$tmp_root/tests/fixtures/stage5/stage5cj_paper_broker_lifecycle.json"
run_negative_case "stage5c-api-freeze-manifest-drift" '# stage5c negative manifest drift' "$tmp_root/docs/stage-5/stage-5c-api-freeze-manifest.json"

stage5c_manifest="$tmp_root/docs/stage-5/stage-5c-api-freeze-manifest.json"
run_manifest_json_mutation_case() {
  local case_name="$1"
  local mutation="$2"
  local backup="$stage5c_manifest.bak"

  cp "$stage5c_manifest" "$backup"
  MUTATION="$mutation" STAGE5C_MANIFEST="$stage5c_manifest" python3 - <<'PY'
import json
import os
from pathlib import Path

path = Path(os.environ["STAGE5C_MANIFEST"])
mutation = os.environ["MUTATION"]
manifest = json.loads(path.read_text())

if mutation == "empty_evidence_map":
    manifest["executable_evidence_map"] = []
elif mutation == "remove_evidence_transition":
    manifest["executable_evidence_map"] = manifest["executable_evidence_map"][1:]
elif mutation == "remove_source_hash_path":
    manifest["source_hashes"].pop("crates/strategy-runtime-core/src/lib.rs")
elif mutation == "alter_baseline_full_commit":
    manifest["accepted_implementation_baseline"]["full_commit"] = "deadbeef"
elif mutation == "alter_baseline_handoff_sha256":
    manifest["accepted_implementation_baseline"]["handoff_sha256"] = "0" * 64
elif mutation == "remove_accepted_slice":
    manifest["accepted_slices"] = manifest["accepted_slices"][:-1]
elif mutation == "remove_public_type":
    manifest["public_types"] = [
        item
        for item in manifest["public_types"]
        if item["name"] != "Stage5cTimerSettlement"
    ]
elif mutation == "remove_public_method":
    manifest["public_methods"] = [
        item
        for item in manifest["public_methods"]
        if not (
            item["type"] == "Stage5cTimerSettlement"
            and item["name"] == "is_ready_for_continuation"
        )
    ]
else:
    raise SystemExit(f"unknown mutation: {mutation}")

path.write_text(json.dumps(manifest, indent=2) + "\n")
PY
  expect_scanner_failure "$case_name"
  mv "$backup" "$stage5c_manifest"
}

run_manifest_json_mutation_case "stage5c-empty-evidence-map-valid-json" "empty_evidence_map"
run_manifest_json_mutation_case "stage5c-remove-evidence-transition-valid-json" "remove_evidence_transition"
run_manifest_json_mutation_case "stage5c-remove-source-hash-path-valid-json" "remove_source_hash_path"
run_manifest_json_mutation_case "stage5c-alter-baseline-full-commit-valid-json" "alter_baseline_full_commit"
run_manifest_json_mutation_case "stage5c-alter-baseline-handoff-sha-valid-json" "alter_baseline_handoff_sha256"
run_manifest_json_mutation_case "stage5c-remove-accepted-slice-valid-json" "remove_accepted_slice"
run_manifest_json_mutation_case "stage5c-remove-public-type-valid-json" "remove_public_type"
run_manifest_json_mutation_case "stage5c-remove-public-method-valid-json" "remove_public_method"

semantic_ledger="$tmp_root/crates/strategy-runtime-core/source-correspondence.toml"
semantic_target="$tmp_root/crates/strategy-runtime-core/src/hybrid_intraday/intraday_breakout.rs"
semantic_ledger_backup="$semantic_ledger.bak"
semantic_target_backup="$semantic_target.bak"
cp "$semantic_ledger" "$semantic_ledger_backup"
cp "$semantic_target" "$semantic_target_backup"
perl -0pi -e 's/k: 0\.65,/k: 0.66,/' "$semantic_target"
changed_target_sha="$(shasum -a 256 "$semantic_target" | awk '{print $1}')"
perl -0pi -e 's/target_sha256 = "a3b125f282f201b66dfa8d2685f22aa94048856a5145d537b76dc8934a5f9ae5"/target_sha256 = "'"$changed_target_sha"'"/' "$semantic_ledger"
expect_scanner_failure "semantic-formula-drift-with-ledger-rehash"
mv "$semantic_ledger_backup" "$semantic_ledger"
mv "$semantic_target_backup" "$semantic_target"

semantic_ledger_backup="$semantic_ledger.bak"
cp "$semantic_ledger" "$semantic_ledger_backup"
perl -0pi -e 's/alor_source_commit = "43242c89944d335d9cb0729b38bdd7d658378d5e"/alor_source_commit = "deadbeef"/' "$semantic_ledger"
expect_scanner_failure "semantic-source-commit-drift"
mv "$semantic_ledger_backup" "$semantic_ledger"

root_manifest="$tmp_root/Cargo.toml"
root_manifest_backup="$root_manifest.bak"
cp "$root_manifest" "$root_manifest_backup"
perl -0pi -e 's/\s*"crates\/strategy-runtime-core",\n//' "$root_manifest"
expect_scanner_failure "remove-strategy-runtime-core-from-workspace"
mv "$root_manifest_backup" "$root_manifest"

cp "$root_manifest" "$root_manifest_backup"
perl -0pi -e 's/^    "crates\/strategy-runtime-core",$/    # "crates\/strategy-runtime-core",/m' "$root_manifest"
expect_scanner_failure "comment-out-workspace-member-but-leave-quoted-comment"
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
semantic_wrapper="$tmp_root/crates/strategy-runtime-core/src/hybrid_intraday_runtime_alias.rs"
cp "$semantic_lib" "$semantic_lib_backup"
printf '\npub struct HybridIntradayRuntimeStrategy;\n' > "$semantic_wrapper"
printf '\npub mod hybrid_intraday_runtime_alias;\n' >> "$semantic_lib"
expect_scanner_failure "add-alternate-stage5b2-wrapper-and-export"
rm -f "$semantic_wrapper"
mv "$semantic_lib_backup" "$semantic_lib"

semantic_manifest_backup="$semantic_manifest.bak"
cp "$semantic_manifest" "$semantic_manifest_backup"
perl -0pi -e 's/\[package\]/[package]\nautotests = false/' "$semantic_manifest"
expect_scanner_failure "disable-strategy-runtime-core-tests"
mv "$semantic_manifest_backup" "$semantic_manifest"

semantic_build_script="$tmp_root/crates/strategy-runtime-core/build.rs"
printf 'fn main() { println!("cargo:rustc-cfg=stage5_semantic_override"); }\n' > "$semantic_build_script"
expect_scanner_failure "add-default-build-script"
rm -f "$semantic_build_script"

semantic_integration_wrapper="$tmp_root/crates/strategy-runtime-core/tests/hybrid_intraday_runtime.rs"
printf 'include!("../../../source-oracles/alor-stage5/hybrid_intraday_runtime.rs");\n' > "$semantic_integration_wrapper"
expect_scanner_failure "add-untracked-integration-wrapper-target"
rm -f "$semantic_integration_wrapper"

semantic_bench_dir="$tmp_root/crates/strategy-runtime-core/benches"
semantic_bench_wrapper="$semantic_bench_dir/hybrid_intraday_runtime.rs"
mkdir -p "$semantic_bench_dir"
printf 'include!("../../../source-oracles/alor-stage5/hybrid_intraday_runtime.rs");\n' > "$semantic_bench_wrapper"
expect_scanner_failure "add-untracked-bench-wrapper-target"
rm -rf "$semantic_bench_dir"

semantic_example_dir="$tmp_root/crates/strategy-runtime-core/examples"
semantic_example_wrapper="$semantic_example_dir/hybrid_intraday_runtime.rs"
mkdir -p "$semantic_example_dir"
printf 'include!("../../../source-oracles/alor-stage5/hybrid_intraday_runtime.rs");\n' > "$semantic_example_wrapper"
expect_scanner_failure "add-untracked-example-wrapper-target"
rm -rf "$semantic_example_dir"

alternate_wrapper="$tmp_root/crates/broker-core/src/hybrid_intraday_runtime.rs"
alternate_lib="$tmp_root/crates/broker-core/src/lib.rs"
alternate_lib_backup="$alternate_lib.bak"
cp "$alternate_lib" "$alternate_lib_backup"
printf 'pub struct\nHybridIntradayRuntimeStrategy;\n' > "$alternate_wrapper"
printf '\npub mod hybrid_intraday_runtime;\n' >> "$alternate_lib"
expect_scanner_failure "copy-wrapper-to-another-workspace-crate-and-export"
rm -f "$alternate_wrapper"
mv "$alternate_lib_backup" "$alternate_lib"

external_member="$tmp_root/stage5-wrapper"
mkdir -p "$external_member/src"
printf '[package]\nname = "stage5-wrapper"\nversion = "0.0.0"\nedition = "2021"\n' > "$external_member/Cargo.toml"
printf 'pub struct HybridIntradayRuntimeStrategy;\n' > "$external_member/src/lib.rs"
cp "$root_manifest" "$root_manifest_backup"
perl -0pi -e 's/    "crates\/strategy-runtime-core",/    "crates\/strategy-runtime-core",\n    "stage5-wrapper",/' "$root_manifest"
expect_scanner_failure "add-wrapper-in-new-workspace-member-outside-crates"
mv "$root_manifest_backup" "$root_manifest"
rm -rf "$external_member"

cp "$root_manifest" "$root_manifest_backup"
perl -0pi -e 's/]\n\n\[workspace\.package\]/]\nexclude = ["stage5-wrapper"]\n\n[workspace.package]/' "$root_manifest"
expect_scanner_failure "workspace-exclude-drift"
mv "$root_manifest_backup" "$root_manifest"

external_member="$tmp_root/stage5-wrapper"
broker_core_manifest="$tmp_root/crates/broker-core/Cargo.toml"
broker_core_manifest_backup="$broker_core_manifest.bak"
mkdir -p "$external_member/src"
printf '%s\n' \
  '[package]' \
  'name = "stage5-wrapper"' \
  'version = "0.0.0"' \
  'edition = "2021"' > "$external_member/Cargo.toml"
printf '%s\n' \
  'pub struct HybridIntradayRuntimeStrategy;' \
  'pub use HybridIntradayRuntimeStrategy as Runtime;' > "$external_member/src/lib.rs"
cp "$broker_core_manifest" "$broker_core_manifest_backup"
printf '\nstage5-wrapper = { path = "../../stage5-wrapper" }\n' >> "$broker_core_manifest"
expect_scanner_failure "unapproved-path-dependency-edge"
mv "$broker_core_manifest_backup" "$broker_core_manifest"
rm -rf "$external_member"

external_member="$tmp_root/stage5-wrapper"
mkdir -p "$external_member/src"
printf '%s\n' \
  '[package]' \
  'name = "stage5-wrapper"' \
  'version = "0.0.0"' \
  'edition = "2021"' > "$external_member/Cargo.toml"
printf '%s\n' \
  'pub struct HybridIntradayRuntimeStrategy;' \
  'pub use HybridIntradayRuntimeStrategy as Runtime;' > "$external_member/src/lib.rs"
cp "$root_manifest" "$root_manifest_backup"
cp "$broker_core_manifest" "$broker_core_manifest_backup"
perl -0pi -e 's/]\n\n\[workspace\.package\]/]\nexclude = ["stage5-wrapper"]\n\n[workspace.package]/' "$root_manifest"
printf '\nstage5-wrapper = { path = "../../stage5-wrapper" }\n' >> "$broker_core_manifest"
expect_scanner_failure "excluded-local-path-dependency-wrapper"
mv "$root_manifest_backup" "$root_manifest"
mv "$broker_core_manifest_backup" "$broker_core_manifest"
rm -rf "$external_member"

build_script="$tmp_root/crates/broker-core/build.rs"
printf 'fn main() {}\n' > "$build_script"
expect_scanner_failure "workspace-member-build-rs"
rm -f "$build_script"

mkdir -p "$tmp_root/.cargo"
printf '%s\n' \
  '[build]' \
  'rustc-wrapper = "synthetic-wrapper"' > "$tmp_root/.cargo/config.toml"
expect_scanner_failure "repository-local-cargo-config"
rm -rf "$tmp_root/.cargo"

broker_cli_manifest="$tmp_root/crates/broker-cli/Cargo.toml"
broker_cli_manifest_backup="$broker_cli_manifest.bak"
cp "$broker_cli_manifest" "$broker_cli_manifest_backup"
perl -0pi -e 's#path = "src/main.rs"#path = "../broker-core/src/lib.rs"#' "$broker_cli_manifest"
expect_scanner_failure "explicit-target-escapes-declaring-member"
mv "$broker_cli_manifest_backup" "$broker_cli_manifest"

alias_fixture_dir="$tmp_root/crates/broker-core/fixtures"
mkdir -p "$alias_fixture_dir"
cp "$tmp_root/source-oracles/alor-stage5/hybrid_intraday_runtime.rs" \
  "$alias_fixture_dir/stage5_wrapper_alias.txt"
expect_scanner_failure "duplicate-oracle-under-alias-filename"
rm -f "$alias_fixture_dir/stage5_wrapper_alias.txt"

printf 'pub struct HybridIntradayRuntimeStrategy;\n' \
  > "$alias_fixture_dir/stage5_wrapper.inc"
cp "$alternate_lib" "$alternate_lib_backup"
macro_alias_path="$tmp_root/crates/broker-core/src/stage5_macro_alias_path.rs"
printf '%s\n' \
  'macro_rules! make_module {' \
  '    ($meta:meta) => {' \
  '        #[$meta]' \
  '        mod stage5_wrapper;' \
  '    };' \
  '}' \
  'make_module!(path = "../fixtures/stage5_wrapper.inc");' \
  > "$macro_alias_path"
printf '\npub mod stage5_macro_alias_path;\n' >> "$alternate_lib"
expect_scanner_failure "macro-meta-path-to-renamed-wrapper-inc"
rm -f "$macro_alias_path" "$alias_fixture_dir/stage5_wrapper.inc"
mv "$alternate_lib_backup" "$alternate_lib"
rmdir "$alias_fixture_dir"

alternate_lib_backup="$alternate_lib.bak"
cp "$alternate_lib" "$alternate_lib_backup"
comment_wrapper="$tmp_root/crates/broker-core/src/stage5_comment_wrapper.rs"
printf 'pub struct /* bypass */ HybridIntradayRuntimeStrategy;\n' > "$comment_wrapper"
printf '\npub mod stage5_comment_wrapper;\n' >> "$alternate_lib"
expect_scanner_failure "comment-separated-wrapper-definition"
rm -f "$comment_wrapper"
mv "$alternate_lib_backup" "$alternate_lib"

cp "$alternate_lib" "$alternate_lib_backup"
macro_wrapper="$tmp_root/crates/broker-core/src/stage5_macro_wrapper.rs"
printf 'macro_rules! make_wrapper { ($name:ident) => { pub struct $name; }; }\nmake_wrapper!(HybridIntradayRuntimeStrategy);\n' > "$macro_wrapper"
printf '\npub mod stage5_macro_wrapper;\n' >> "$alternate_lib"
expect_scanner_failure "macro-generated-wrapper-definition"
rm -f "$macro_wrapper"
mv "$alternate_lib_backup" "$alternate_lib"

cp "$alternate_lib" "$alternate_lib_backup"
include_wrapper="$tmp_root/crates/broker-core/src/stage5_include_wrapper.rs"
printf 'include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../source-oracles/alor-stage5/hybrid_intraday_runtime.rs"));\n' > "$include_wrapper"
printf '\npub mod stage5_include_wrapper;\n' >> "$alternate_lib"
expect_scanner_failure "include-wrapper-oracle"
rm -f "$include_wrapper"
mv "$alternate_lib_backup" "$alternate_lib"

cp "$alternate_lib" "$alternate_lib_backup"
split_include_wrapper="$tmp_root/crates/broker-core/src/stage5_split_include.rs"
printf 'include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../source-oracles/alor-stage5/", "hybrid_intraday_", "runtime.rs"));\n' > "$split_include_wrapper"
printf '\npub mod stage5_split_include;\n' >> "$alternate_lib"
expect_scanner_failure "split-path-include-wrapper-oracle"
rm -f "$split_include_wrapper"
mv "$alternate_lib_backup" "$alternate_lib"

cp "$alternate_lib" "$alternate_lib_backup"
generic_include="$tmp_root/crates/broker-core/src/stage5_generic_include.rs"
printf 'include!("synthetic_generated.rs");\n' > "$generic_include"
printf '\npub mod stage5_generic_include;\n' >> "$alternate_lib"
expect_scanner_failure "any-include-macro-before-wrapper-gate"
rm -f "$generic_include"
mv "$alternate_lib_backup" "$alternate_lib"

cp "$alternate_lib" "$alternate_lib_backup"
comment_include="$tmp_root/crates/broker-core/src/stage5_comment_include.rs"
printf 'include /* bypass */ ! ("synthetic_generated.rs");\n' > "$comment_include"
printf '\npub mod stage5_comment_include;\n' >> "$alternate_lib"
expect_scanner_failure "comment-separated-include-macro"
rm -f "$comment_include"
mv "$alternate_lib_backup" "$alternate_lib"

cp "$alternate_lib" "$alternate_lib_backup"
nested_comment_include="$tmp_root/crates/broker-core/src/stage5_nested_comment_include.rs"
printf 'include /* outer /* nested */ outer */ ! ("synthetic_generated.rs");\n' > "$nested_comment_include"
printf '\npub mod stage5_nested_comment_include;\n' >> "$alternate_lib"
expect_scanner_failure "nested-comment-separated-include-macro"
rm -f "$nested_comment_include"
mv "$alternate_lib_backup" "$alternate_lib"

cp "$alternate_lib" "$alternate_lib_backup"
raw_identifier_include="$tmp_root/crates/broker-core/src/stage5_raw_identifier_include.rs"
printf 'r#include!("synthetic_generated.rs");\n' > "$raw_identifier_include"
printf '\npub mod stage5_raw_identifier_include;\n' >> "$alternate_lib"
expect_scanner_failure "raw-identifier-include-macro"
rm -f "$raw_identifier_include"
mv "$alternate_lib_backup" "$alternate_lib"

cp "$alternate_lib" "$alternate_lib_backup"
indirect_include="$tmp_root/crates/broker-core/src/stage5_indirect_include.rs"
printf '%s\n' \
  'macro_rules! activate_file {' \
  '    ($loader:ident, $path:expr) => { $loader!($path); };' \
  '}' \
  'activate_file!(include, "synthetic_generated.rs");' > "$indirect_include"
printf '\npub mod stage5_indirect_include;\n' >> "$alternate_lib"
expect_scanner_failure "macro-indirected-include-activation"
rm -f "$indirect_include"
mv "$alternate_lib_backup" "$alternate_lib"

cp "$alternate_lib" "$alternate_lib_backup"
path_wrapper="$tmp_root/crates/broker-core/src/stage5_path_wrapper.rs"
printf '#[path = "../../../source-oracles/alor-stage5/hybrid_intraday_runtime.rs"]\nmod stage5_wrapper;\n' > "$path_wrapper"
printf '\npub mod stage5_path_wrapper;\n' >> "$alternate_lib"
expect_scanner_failure "path-attribute-wrapper-oracle"
rm -f "$path_wrapper"
mv "$alternate_lib_backup" "$alternate_lib"

cp "$alternate_lib" "$alternate_lib_backup"
comment_path="$tmp_root/crates/broker-core/src/stage5_comment_path.rs"
printf '#[/* bypass */ path = "synthetic_generated.rs"]\nmod generated;\n' > "$comment_path"
printf '\npub mod stage5_comment_path;\n' >> "$alternate_lib"
expect_scanner_failure "comment-separated-path-attribute"
rm -f "$comment_path"
mv "$alternate_lib_backup" "$alternate_lib"

cp "$alternate_lib" "$alternate_lib_backup"
cfg_attr_path="$tmp_root/crates/broker-core/src/stage5_cfg_attr_path.rs"
printf '%s\n' \
  '#[cfg_attr(' \
  '    all(),' \
  '    path = "synthetic_generated.rs"' \
  ')]' \
  'mod generated;' > "$cfg_attr_path"
printf '\npub mod stage5_cfg_attr_path;\n' >> "$alternate_lib"
expect_scanner_failure "cfg-attr-path-wrapper-activation"
rm -f "$cfg_attr_path"
mv "$alternate_lib_backup" "$alternate_lib"

cp "$alternate_lib" "$alternate_lib_backup"
macro_meta_path="$tmp_root/crates/broker-core/src/stage5_macro_meta_path.rs"
printf '%s\n' \
  'macro_rules! make_module {' \
  '    ($meta:meta) => {' \
  '        #[$meta]' \
  '        mod stage5_wrapper;' \
  '    };' \
  '}' \
  'make_module!(' \
  '    path = "../../../source-oracles/alor-stage5/hybrid_intraday_runtime.rs"' \
  ');' > "$macro_meta_path"
printf '\npub mod stage5_macro_meta_path;\n' >> "$alternate_lib"
expect_scanner_failure "macro-meta-path-wrapper-activation"
rm -f "$macro_meta_path"
mv "$alternate_lib_backup" "$alternate_lib"

cp "$alternate_lib" "$alternate_lib_backup"
split_oracle_read="$tmp_root/crates/broker-core/src/stage5_split_oracle_read.rs"
printf '%s\n' \
  'pub const ORACLE: &str = include_str!(concat!(' \
  '    env!("CARGO_MANIFEST_DIR"),' \
  '    "/../../source-oracles/alor-stage5/",' \
  '    "hybrid_intraday_",' \
  '    "runtime.rs"' \
  '));' > "$split_oracle_read"
printf '\npub mod stage5_split_oracle_read;\n' >> "$alternate_lib"
expect_scanner_failure "split-path-oracle-include-str-outside-allowlist"
rm -f "$split_oracle_read"
mv "$alternate_lib_backup" "$alternate_lib"

cp "$alternate_lib" "$alternate_lib_backup"
escaped_oracle_read="$tmp_root/crates/broker-core/src/stage5_escaped_oracle_read.rs"
printf '%s\n' \
  'pub const ORACLE: &str = include_str!(' \
  '    "../../../source-oracles/alor-stage5/hybrid_intraday_\x72untime.rs"' \
  ');' > "$escaped_oracle_read"
printf '\npub mod stage5_escaped_oracle_read;\n' >> "$alternate_lib"
expect_scanner_failure "escaped-oracle-include-str-outside-allowlist"
rm -f "$escaped_oracle_read"
mv "$alternate_lib_backup" "$alternate_lib"

cp "$alternate_lib" "$alternate_lib_backup"
unicode_oracle_read="$tmp_root/crates/broker-core/src/stage5_unicode_oracle_read.rs"
printf '%s\n' \
  'pub const ORACLE: &str = include_str!(' \
  '    "../../../source-oracles/alor-stage5/hybrid_intraday_\u{72}untime.rs"' \
  ');' > "$unicode_oracle_read"
printf '\npub mod stage5_unicode_oracle_read;\n' >> "$alternate_lib"
expect_scanner_failure "unicode-escaped-oracle-filename"
rm -f "$unicode_oracle_read"
mv "$alternate_lib_backup" "$alternate_lib"

cp "$alternate_lib" "$alternate_lib_backup"
stringify_oracle_read="$tmp_root/crates/broker-core/src/stage5_stringify_oracle_read.rs"
printf '%s\n' \
  'pub const ORACLE: &str = include_str!(concat!(' \
  '    env!("CARGO_MANIFEST_DIR"),' \
  '    "/../../source-oracles/alor-stage5/hybrid_intraday_",' \
  '    stringify!(runtime),' \
  '    ".rs"' \
  '));' > "$stringify_oracle_read"
printf '\npub mod stage5_stringify_oracle_read;\n' >> "$alternate_lib"
expect_scanner_failure "stringify-split-oracle-include-str"
rm -f "$stringify_oracle_read"
mv "$alternate_lib_backup" "$alternate_lib"

cp "$alternate_lib" "$alternate_lib_backup"
lexical_non_code="$tmp_root/crates/broker-core/src/stage5_lexical_non_code.rs"
printf '%s\n' \
  '// include /* comment */ ! ("not_code.rs");' \
  '/* #[/* nested */ path = "not_code.rs"] */' \
  'pub const INCLUDE_TEXT: &str = "include!(\"not_code.rs\")";' \
  'pub const ESCAPED_TEXT: &str = "prefix \" include!(\"not_code.rs\")";' \
  'pub const PATH_TEXT: &str = r###"#[path = "not_code.rs"]"###;' \
  "pub const INCLUDE_CHAR: char = '!';" > "$lexical_non_code"
printf '\npub mod stage5_lexical_non_code;\n' >> "$alternate_lib"
expect_scanner_success "include-and-path-text-outside-rust-code"
rm -f "$lexical_non_code"
mv "$alternate_lib_backup" "$alternate_lib"

bracket_fixture="$tmp_root/tests/fixtures/stage5/bracket_terminal_reconciliation.json"
bracket_fixture_backup="$bracket_fixture.bak"
cp "$bracket_fixture" "$bracket_fixture_backup"
perl -0pi -e 's/"grace_ms": 3000/"grace_ms": 4000/' "$bracket_fixture"
expect_scanner_failure "drift-bracket-terminal-reconciliation-fixture"
mv "$bracket_fixture_backup" "$bracket_fixture"

stage5b2_manifest="$tmp_root/crates/strategy-runtime-core/stage5b2-source-correspondence.toml"
stage5b2_manifest_backup="$stage5b2_manifest.bak"
cp "$stage5b2_manifest" "$stage5b2_manifest_backup"
perl -0pi -e 's/wrapper_compiled = true/wrapper_compiled = false/' "$stage5b2_manifest"
expect_scanner_failure "close-stage5b2-wrapper-compiled-milestone"
mv "$stage5b2_manifest_backup" "$stage5b2_manifest"

stage5b2_fixture="$tmp_root/tests/fixtures/stage5/stage5b2_callback_state_mapping.json"
stage5b2_fixture_backup="$stage5b2_fixture.bak"
cp "$stage5b2_fixture" "$stage5b2_fixture_backup"
perl -0pi -e 's/"runtime_host_attached": false/"runtime_host_attached": true/' "$stage5b2_fixture"
expect_scanner_failure "open-stage5b2-runtime-host-boundary"
mv "$stage5b2_fixture_backup" "$stage5b2_fixture"

run_replacement_case \
  "forbidden-harness-marker-check-removal" \
  "$tmp_root/scripts/forbidden_surface_negative_case_worker.sh" \
  'if ! grep -F -- "$expected_marker" "$scan_log" >/dev/null; then' \
  'if false; then'
run_replacement_case \
  "forbidden-harness-inventory-reduction" \
  "$tmp_root/scripts/forbidden_surface_negative_harness.py" \
  '    Case("same-module-extra-post", "broker-finam has unexpected .post( count"),' \
  '    # inventory entry removed by negative mutation'
run_replacement_case \
  "forbidden-worker-contract-drift" \
  "$tmp_root/scripts/forbidden_surface_negative_case_worker.sh" \
  '  check_scanner_result "$case_name" "$exit_code"' \
  '  : # scanner result contract bypassed by negative mutation'
run_replacement_case \
  "forbidden-ci-timeout-lowered" \
  "$tmp_root/.github/workflows/ci.yml" \
  '        timeout-minutes: 75' \
  '        timeout-minutes: 1'
run_replacement_case \
  "forbidden-baseline-positive-bypass" \
  "$tmp_root/scripts/forbidden_surface_negative_harness.py" \
  '        if clean_result.returncode != 0:' \
  '        if False:  # baseline-positive validation bypassed by negative mutation'
run_replacement_case \
  "forbidden-scanner-contract-drift" \
  "$tmp_root/scripts/forbidden_surface_scan.sh" \
  'FORBIDDEN_SURFACE_SCANNER_CONTRACT="stage5d-b2bc1-r4-v1"' \
  'FORBIDDEN_SURFACE_SCANNER_CONTRACT="unreviewed"'

if [[ "$case_executed" -ne 1 ]]; then
  echo "selected case did not execute exactly once: $case_filter count=$case_executed" >&2
  exit 1
fi
echo "forbidden-surface-negative-case: ok name=$case_filter"
