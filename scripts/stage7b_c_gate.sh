#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
artifact_dir="${STAGE7B_C_ARTIFACT_DIR:-${TMPDIR:-/tmp}/stage7b-c-artifact.$$}"
mkdir -p "$artifact_dir"

cargo fmt --all -- --check
echo "fmt: PASS" | tee "$artifact_dir/fmt.txt"
python3 scripts/stage7b_c_check.py | tee "$artifact_dir/stage7b-c-check.txt"
python3 scripts/stage7b_c_closed_surface_check.py | tee "$artifact_dir/closed-surface.txt"
python3 scripts/stage7b_c_negative_harness.py | tee "$artifact_dir/negative.txt"

inherited="$artifact_dir/detached-stage7b-b-r2-ff3fa2e"
git clone --quiet --no-hardlinks --shared . "$inherited"
git -C "$inherited" checkout --quiet -B stage7b-production-durability \
  ff3fa2e8908440863b40b838991d4716b33caad4
STAGE7B_B_ARTIFACT_DIR="$artifact_dir/inherited-stage7b-b-r2-artifacts" \
  bash "$inherited/scripts/stage7b_b_gate.sh" 2>&1 \
  | tee "$artifact_dir/inherited-stage7b-b-r2-gate.txt"
rm -rf "$inherited"

cargo test -p runtime-durable-service --lib recovery::tests -- --nocapture 2>&1 \
  | tee "$artifact_dir/stage7b-c-debug.txt"
cargo test --release -p runtime-durable-service --lib recovery::tests -- --nocapture 2>&1 \
  | tee "$artifact_dir/stage7b-c-release.txt"
cargo test -p strategy-runtime-core --lib stage6e_extra_finalized_stage6_history_does_not_need_current_stage5_slot -- --nocapture 2>&1 \
  | tee "$artifact_dir/stage6-finalized-ahead.txt"
cargo test -p strategy-runtime-core --lib stage6e_extra_unresolved_stage6_authority_is_rejected -- --nocapture 2>&1 \
  | tee "$artifact_dir/stage6-unbound-nonfinal.txt"
cargo test -p strategy-runtime-core --lib stage6e_matching_stage5_stage6_pair_is_cross_bound_before_capability -- --nocapture 2>&1 \
  | tee "$artifact_dir/stage6-cross-bound-active.txt"
cargo test -p strategy-runtime-core --lib stage6 --no-fail-fast 2>&1 \
  | tee "$artifact_dir/stage6-regression.txt"
cargo test --workspace --all-targets 2>&1 | tee "$artifact_dir/workspace-tests.txt"
cargo test --workspace --doc 2>&1 | tee "$artifact_dir/workspace-docs.txt"
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 \
  | tee "$artifact_dir/clippy.txt"
rustc --version | tee "$artifact_dir/toolchain.txt"
cargo --version | tee -a "$artifact_dir/toolchain.txt"
echo "stage7b-c-gate: PASS artifact_dir=$artifact_dir"
