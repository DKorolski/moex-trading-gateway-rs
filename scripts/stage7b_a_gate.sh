#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
artifact_dir="${STAGE7B_A_ARTIFACT_DIR:-${TMPDIR:-/tmp}/stage7b-a-artifact.$$}"
mkdir -p "$artifact_dir"

cargo fmt --all -- --check
echo "fmt: PASS" | tee "$artifact_dir/fmt.txt"
python3 scripts/stage7b_check.py | tee "$artifact_dir/stage7b-check.txt"
python3 scripts/stage7b_closed_surface_check.py | tee "$artifact_dir/closed-surface.txt"
python3 scripts/stage7b_negative_harness.py | tee "$artifact_dir/negative.txt"

inherited="$artifact_dir/detached-stage7a-2b6d6e9"
git clone --quiet --no-hardlinks --shared . "$inherited"
git -C "$inherited" checkout --quiet -B stage7a-paper-command-consumer \
  2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64
STAGE7A_ARTIFACT_DIR="$artifact_dir/inherited-stage7a-artifacts" \
  bash "$inherited/scripts/stage7a_gate.sh" 2>&1 \
  | tee "$artifact_dir/inherited-stage7a-gate.txt"
rm -rf "$inherited"

cargo test -p strategy-runtime-core stage7b -- --nocapture 2>&1 \
  | tee "$artifact_dir/stage7b-core-debug.txt"
cargo test --release -p strategy-runtime-core stage7b -- --nocapture 2>&1 \
  | tee "$artifact_dir/stage7b-core-release.txt"
cargo test -p strategy-runtime-core --lib stage6 --no-fail-fast 2>&1 \
  | tee "$artifact_dir/stage6-regression.txt"
cargo test --workspace --all-targets 2>&1 | tee "$artifact_dir/workspace-tests.txt"
cargo test --workspace --doc 2>&1 | tee "$artifact_dir/workspace-docs.txt"
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 \
  | tee "$artifact_dir/clippy.txt"
rustc --version | tee "$artifact_dir/toolchain.txt"
cargo --version | tee -a "$artifact_dir/toolchain.txt"
echo "stage7b-a-gate: PASS artifact_dir=$artifact_dir"
