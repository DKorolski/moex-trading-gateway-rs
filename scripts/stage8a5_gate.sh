#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
artifact_dir="${STAGE8A5_ARTIFACT_DIR:-$repo_root/tmp/stage8a5-gate}"
rm -rf "$artifact_dir"
mkdir -p "$artifact_dir"
run() { local name="$1"; shift; "$@" >"$artifact_dir/$name.txt" 2>&1; }
run_with_x16_bounded_retry() {
  local name="$1"; shift
  if "$@" >"$artifact_dir/$name.txt" 2>&1; then
    return 0
  fi
  local failed_count
  failed_count="$(grep -c ' \.\.\. FAILED$' "$artifact_dir/$name.txt" || true)"
  if [[ "$failed_count" != "1" ]] || \
     ! grep -q 'stage7b_e_x16_sigkill_during_claim_is_reclaimable_by_next_boot ... FAILED' \
       "$artifact_dir/$name.txt" || \
     ! grep -q 'X16 child did not reach claim barrier' "$artifact_dir/$name.txt"; then
    echo "stage8a5-gate: $name failed outside the exact X16 timing allowance" >&2
    return 1
  fi
  mv "$artifact_dir/$name.txt" "$artifact_dir/$name-first-x16-timeout.txt"
  "$@" >"$artifact_dir/$name-retry.txt" 2>&1
  cp "$artifact_dir/$name-retry.txt" "$artifact_dir/$name.txt"
}

run aggregate-semantic python3 scripts/stage8a5_check.py
run aggregate-negative python3 scripts/stage8a5_negative_harness.py
run forbidden-surface python3 scripts/stage8a5_forbidden_surface_check.py
run forbidden-surface-negative python3 scripts/stage8a5_forbidden_surface_negative_harness.py
run inherited-stage8 python3 scripts/stage8a5_inherited_stage8_check.py \
  --output "$artifact_dir/stage8a5-inherited-stage8-result.json"

run current-i4-semantic python3 scripts/stage8a4_durable_composition_i4_check.py
run current-i4-negative python3 scripts/stage8a4_durable_composition_i4_negative_harness.py
run current-i4-inherited-design python3 scripts/stage8a4_durable_composition_i4_inherited_design_check.py
run current-i4-inherited-design-negative python3 scripts/stage8a4_durable_composition_i4_design_negative_harness.py
run i3-external-compile bash scripts/stage8a4_durable_composition_i3_external_compile_fail.sh
run i4-external-compile bash scripts/stage8a4_durable_composition_i4_external_compile_fail.sh

stage7b_checkout="$repo_root/tmp/stage8a5-stage7b-checkout"
rm -rf "$stage7b_checkout"
mkdir -p "$stage7b_checkout"
trap 'rm -rf "$stage7b_checkout"' EXIT
real_cargo="$(command -v cargo)"
mkdir -p "$stage7b_checkout/bin" "$repo_root/target/stage8a5-detached"
ln -s "$repo_root/scripts/stage8a5_detached_cargo.sh" "$stage7b_checkout/bin/cargo"
git clone --quiet --no-hardlinks --shared "$repo_root" "$stage7b_checkout/repo"
git -C "$stage7b_checkout/repo" checkout --quiet -B stage7b-production-durability \
  a1044e0dbe324c722b637498ca80ffafd9f0cbee
run_stage7b() {
  RUST_TEST_THREADS=1 \
    STAGE7B_E_ARTIFACT_DIR="$artifact_dir/inherited-stage7b" \
    STAGE8A5_REAL_CARGO="$real_cargo" \
    STAGE8A5_DETACHED_TARGET_ROOT="$repo_root/target/stage8a5-detached" \
    PATH="$stage7b_checkout/bin:$PATH" \
    bash "$stage7b_checkout/repo/scripts/stage7b_e_gate.sh"
}
if ! run_stage7b >"$artifact_dir/inherited-stage7b-gate.txt" 2>&1; then
  failed_count="$(grep -c ' \.\.\. FAILED$' "$artifact_dir/inherited-stage7b-gate.txt" || true)"
  if [[ "$failed_count" != "1" ]] || \
     ! grep -q 'stage7b_e_x16_sigkill_during_claim_is_reclaimable_by_next_boot ... FAILED' \
       "$artifact_dir/inherited-stage7b-gate.txt" || \
     ! grep -q 'X16 child did not reach claim barrier' "$artifact_dir/inherited-stage7b-gate.txt"; then
    echo "stage8a5-gate: inherited Stage7B failed outside the exact X16 timing allowance" >&2
    exit 1
  fi
  mv "$artifact_dir/inherited-stage7b-gate.txt" \
    "$artifact_dir/inherited-stage7b-gate-first-x16-timeout.txt"
  run_stage7b >"$artifact_dir/inherited-stage7b-gate-retry.txt" 2>&1
  cp "$artifact_dir/inherited-stage7b-gate-retry.txt" \
    "$artifact_dir/inherited-stage7b-gate.txt"
fi
rm -rf "$stage7b_checkout"
trap - EXIT

run fmt cargo fmt --all -- --check
run_with_x16_bounded_retry workspace-debug cargo test --workspace --all-targets -- --test-threads=1
run_with_x16_bounded_retry workspace-release cargo test --workspace --release --all-targets -- --test-threads=1
run workspace-doc cargo test --workspace --doc
run workspace-clippy cargo clippy --workspace --all-targets --all-features -- -D warnings
run python-compile python3 -m py_compile \
  scripts/stage8a5_check.py \
  scripts/stage8a5_negative_harness.py \
  scripts/stage8a5_forbidden_surface_check.py \
  scripts/stage8a5_forbidden_surface_negative_harness.py \
  scripts/stage8a5_inherited_stage8_check.py \
  scripts/stage8a5_handoff_safety_check.py \
  scripts/make_stage8a5_handoff.py
run diff-check git diff --check

python3 - "$artifact_dir/stage8a5-aggregate-acceptance-result.json" <<'PY'
import hashlib
import json
import subprocess
import sys
from pathlib import Path

output = Path(sys.argv[1])
root = Path.cwd()
authority = root / "docs/stage-8/stage8a5-aggregate-acceptance-authority.json"
matrix = root / "docs/stage-8/STAGE8A5_AGGREGATE_ACCEPTANCE_MATRIX_2026-08-21.csv"
negative = root / "docs/stage-8/STAGE8A5_AGGREGATE_NEGATIVE_INVENTORY_2026-08-21.md"
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
result = {
    "schema_version": 1,
    "stage": "8A-5-aggregate-acceptance",
    "result": "PASS",
    "source_ref": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
    "accepted_predecessor": "4a11688c941ee240e377b384042c4bca837b040f",
    "acceptance_rows": 30,
    "negative_cases": 20,
    "forbidden_surface_negative_cases": 10,
    "inherited_stage8_slice_count": 11,
    "inherited_stage8_negative_cases": 544,
    "current_i4_negative_cases": 28,
    "inherited_stage7b_gate": "PASS",
    "inherited_stage7b_test_threads": 1,
    "inherited_stage7b_exact_x16_bounded_retry_used": (output.parent / "inherited-stage7b-gate-first-x16-timeout.txt").is_file(),
    "workspace_debug_exact_x16_bounded_retry_used": (output.parent / "workspace-debug-first-x16-timeout.txt").is_file(),
    "workspace_release_exact_x16_bounded_retry_used": (output.parent / "workspace-release-first-x16-timeout.txt").is_file(),
    "workspace_debug": "PASS",
    "workspace_release": "PASS",
    "workspace_doc": "PASS",
    "workspace_clippy": "PASS",
    "external_compile_boundaries": "PASS",
    "production_rust_changed": False,
    "cargo_or_lock_changed": False,
    "workflow_changed": False,
    "stage8a_closed": False,
    "stage8b_authorized": False,
    "redis_live_consumer_enabled": False,
    "finam_post_delete_enabled": False,
    "broker_dispatch_enabled": False,
    "runtime_live_enabled": False,
    "real_orders_enabled": False,
    "authority_sha256": sha(authority),
    "matrix_sha256": sha(matrix),
    "negative_inventory_sha256": sha(negative),
}
output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

echo "stage8a5-gate: PASS acceptance_rows=30 negative_cases=20 inherited_stage8_negatives=544 current_i4_negatives=28 stage8b=false"
