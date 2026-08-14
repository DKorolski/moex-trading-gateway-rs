#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
artifact_dir="${STAGE7B_E_ARTIFACT_DIR:-${TMPDIR:-/tmp}/stage7b-e-artifact.$$}"
mkdir -p "$artifact_dir"

command -v redis-server >/dev/null
redis-server --version | tee "$artifact_dir/redis-toolchain.txt"
cargo fmt --all -- --check
echo "fmt: PASS" | tee "$artifact_dir/fmt.txt"
python3 scripts/stage7b_e_check.py | tee "$artifact_dir/stage7b-e-check.txt"
python3 scripts/stage7b_e_negative_harness.py | tee "$artifact_dir/stage7b-e-negative.txt"

inherited="$artifact_dir/detached-stage7b-d-c-r2"
git clone --quiet --no-hardlinks . "$inherited"
git -C "$inherited" checkout --quiet -B stage7b-production-durability \
  2b6371adb905654e0ddd8b6714159bcef737b577
STAGE7B_D_C_ARTIFACT_DIR="$artifact_dir/inherited-d-c-artifacts" \
  bash "$inherited/scripts/stage7b_d_c_gate.sh" 2>&1 \
  | tee "$artifact_dir/inherited-d-c-gate.txt"
rm -rf "$inherited"

cargo test -p runtime-durable-service -- --nocapture 2>&1 \
  | tee "$artifact_dir/runtime-debug.txt"
cargo test --release -p runtime-durable-service -- --nocapture 2>&1 \
  | tee "$artifact_dir/runtime-release.txt"
{
  cargo test -p strategy-runtime-core stage7b_ -- --nocapture
  cargo test -p strategy-runtime-core stage6b_ -- --nocapture
} 2>&1 | tee "$artifact_dir/core-debug.txt"
{
  cargo test --release -p strategy-runtime-core stage7b_ -- --nocapture
  cargo test --release -p strategy-runtime-core stage6b_ -- --nocapture
} 2>&1 | tee "$artifact_dir/core-release.txt"

python3 scripts/stage7b_fault_matrix_check.py \
  --artifact-dir "$artifact_dir" \
  --output "$artifact_dir/stage7b-fault-matrix-result.json" \
  | tee "$artifact_dir/fault-matrix.txt"

cargo test --workspace --all-targets 2>&1 | tee "$artifact_dir/workspace-tests.txt"
cargo test --workspace --doc 2>&1 | tee "$artifact_dir/workspace-docs.txt"
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 \
  | tee "$artifact_dir/clippy.txt"
rustc --version | tee "$artifact_dir/toolchain.txt"
cargo --version | tee -a "$artifact_dir/toolchain.txt"
python3 scripts/stage7b_acceptance_report.py \
  --artifact-dir "$artifact_dir" \
  --output "$artifact_dir/stage7b-acceptance-result.json" \
  | tee "$artifact_dir/acceptance-report.txt"
echo "stage7b-e-gate: PASS artifact_dir=$artifact_dir"
