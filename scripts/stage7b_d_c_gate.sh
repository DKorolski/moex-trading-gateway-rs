#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
artifact_dir="${STAGE7B_D_C_ARTIFACT_DIR:-${TMPDIR:-/tmp}/stage7b-d-c-artifact.$$}"
mkdir -p "$artifact_dir"

command -v redis-server >/dev/null
redis-server --version | tee "$artifact_dir/redis-toolchain.txt"
cargo fmt --all -- --check
echo "fmt: PASS" | tee "$artifact_dir/fmt.txt"
python3 scripts/stage7b_d_c_check.py | tee "$artifact_dir/stage7b-d-c-check.txt"
python3 scripts/stage7b_d_c_negative_harness.py | tee "$artifact_dir/negative.txt"

inherited="$artifact_dir/detached-stage7b-d-b-r1"
git clone --quiet --no-hardlinks . "$inherited"
git -C "$inherited" checkout --quiet -B stage7b-production-durability \
  e0bf9b7d9eb209e19b875f199511a493ddcd0da9
STAGE7B_D_B_ARTIFACT_DIR="$artifact_dir/inherited-d-b-artifacts" \
  bash "$inherited/scripts/stage7b_d_b_gate.sh" 2>&1 \
  | tee "$artifact_dir/inherited-d-b-gate.txt"
rm -rf "$inherited"

cargo test -p runtime-durable-service stage7b_d_c -- --nocapture 2>&1 \
  | tee "$artifact_dir/stage7b-d-c-debug.txt"
cargo test --release -p runtime-durable-service stage7b_d_c -- --nocapture 2>&1 \
  | tee "$artifact_dir/stage7b-d-c-release.txt"
cargo test --workspace --all-targets 2>&1 | tee "$artifact_dir/workspace-tests.txt"
cargo test --workspace --doc 2>&1 | tee "$artifact_dir/workspace-docs.txt"
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 \
  | tee "$artifact_dir/clippy.txt"
rustc --version | tee "$artifact_dir/toolchain.txt"
cargo --version | tee -a "$artifact_dir/toolchain.txt"
echo "stage7b-d-c-gate: PASS artifact_dir=$artifact_dir"
