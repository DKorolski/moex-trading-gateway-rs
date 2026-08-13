#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
artifact_dir="${STAGE7B_D_A_ARTIFACT_DIR:-${TMPDIR:-/tmp}/stage7b-d-a-artifact.$$}"
mkdir -p "$artifact_dir"

cargo fmt --all -- --check
echo "fmt: PASS" | tee "$artifact_dir/fmt.txt"
python3 scripts/stage7b_d_a_check.py | tee "$artifact_dir/stage7b-d-a-check.txt"
python3 scripts/stage7b_d_a_negative_harness.py | tee "$artifact_dir/negative.txt"

inherited="$artifact_dir/detached-stage7b-d-design-r1"
git clone --quiet --no-hardlinks . "$inherited"
git -C "$inherited" checkout --quiet -B stage7b-production-durability \
  00cead2989493b44e0d86ead29b95d57a7fbcbe2
STAGE7B_D_DESIGN_ARTIFACT_DIR="$artifact_dir/inherited-design-artifacts" \
  bash "$inherited/scripts/stage7b_d_design_gate.sh" 2>&1 \
  | tee "$artifact_dir/inherited-design-gate.txt"
rm -rf "$inherited"

cargo test -p runtime-durable-service stage7b_d_a -- --nocapture 2>&1 \
  | tee "$artifact_dir/stage7b-d-a-debug.txt"
cargo test --release -p runtime-durable-service stage7b_d_a -- --nocapture 2>&1 \
  | tee "$artifact_dir/stage7b-d-a-release.txt"
cargo test --workspace --all-targets 2>&1 | tee "$artifact_dir/workspace-tests.txt"
cargo test --workspace --doc 2>&1 | tee "$artifact_dir/workspace-docs.txt"
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 \
  | tee "$artifact_dir/clippy.txt"
rustc --version | tee "$artifact_dir/toolchain.txt"
cargo --version | tee -a "$artifact_dir/toolchain.txt"
echo "stage7b-d-a-gate: PASS artifact_dir=$artifact_dir"
