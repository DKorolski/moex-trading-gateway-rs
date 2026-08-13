#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
artifact_dir="${STAGE7B_C_R1_ARTIFACT_DIR:-${TMPDIR:-/tmp}/stage7b-c-r1-artifact.$$}"
mkdir -p "$artifact_dir"

cargo fmt --all -- --check
echo "fmt: PASS" | tee "$artifact_dir/fmt.txt"
python3 scripts/stage7b_c_check.py | tee "$artifact_dir/stage7b-c-r1-check.txt"
python3 scripts/stage7b_c_closed_surface_check.py | tee "$artifact_dir/closed-surface.txt"
python3 scripts/stage7b_c_negative_harness.py | tee "$artifact_dir/negative.txt"

inherited="$artifact_dir/detached-stage7b-c-3d443be"
# Use a self-contained object store here: the inherited C gate recursively
# creates its own B/A/6E review clones, and chained alternates eventually hit
# Git's nesting limit.
git clone --quiet --no-hardlinks . "$inherited"
git -C "$inherited" checkout --quiet -B stage7b-production-durability \
  3d443be72b8a6eb24d1295c800849d11789dba6f
STAGE7B_C_ARTIFACT_DIR="$artifact_dir/inherited-stage7b-c-artifacts" \
  bash "$inherited/scripts/stage7b_c_gate.sh" 2>&1 \
  | tee "$artifact_dir/inherited-stage7b-c-gate.txt"
rm -rf "$inherited"

cargo test -p runtime-durable-service --lib recovery::tests -- --nocapture 2>&1 \
  | tee "$artifact_dir/stage7b-c-r1-debug.txt"
cargo test --release -p runtime-durable-service --lib recovery::tests -- --nocapture 2>&1 \
  | tee "$artifact_dir/stage7b-c-r1-release.txt"
cargo test -p runtime-durable-service --lib stage7b_c_b0 -- --nocapture 2>&1 \
  | tee "$artifact_dir/stage7b-c-r1-direct-witnesses.txt"
cargo test --workspace --all-targets 2>&1 | tee "$artifact_dir/workspace-tests.txt"
cargo test --workspace --doc 2>&1 | tee "$artifact_dir/workspace-docs.txt"
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 \
  | tee "$artifact_dir/clippy.txt"
rustc --version | tee "$artifact_dir/toolchain.txt"
cargo --version | tee -a "$artifact_dir/toolchain.txt"
echo "stage7b-c-r1-gate: PASS artifact_dir=$artifact_dir"
