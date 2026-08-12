#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
artifact_dir="${STAGE7B_B_ARTIFACT_DIR:-${TMPDIR:-/tmp}/stage7b-b-artifact.$$}"
mkdir -p "$artifact_dir"

cargo fmt --all -- --check
echo "fmt: PASS" | tee "$artifact_dir/fmt.txt"
python3 scripts/stage7b_b_check.py | tee "$artifact_dir/stage7b-b-check.txt"
python3 scripts/stage7b_b_closed_surface_check.py | tee "$artifact_dir/closed-surface.txt"
python3 scripts/stage7b_b_negative_harness.py | tee "$artifact_dir/negative.txt"

inherited="$artifact_dir/detached-stage7b-a-r1-a947c24"
git clone --quiet --no-hardlinks --shared . "$inherited"
git -C "$inherited" checkout --quiet --detach \
  a947c24bb413a91c5eb0ad97f4ac0b402bfd0641
STAGE7B_A_ARTIFACT_DIR="$artifact_dir/inherited-stage7b-a-r1-artifacts" \
  bash "$inherited/scripts/stage7b_a_gate.sh" 2>&1 \
  | tee "$artifact_dir/inherited-stage7b-a-r1-gate.txt"
rm -rf "$inherited"

cargo test -p strategy-runtime-core stage7b -- --nocapture 2>&1 \
  | tee "$artifact_dir/stage7b-core-debug.txt"
cargo test -p runtime-durable-service stage7b_b -- --nocapture 2>&1 \
  | tee "$artifact_dir/stage7b-b-service-debug.txt"
cargo test --release -p runtime-durable-service stage7b_b -- --nocapture 2>&1 \
  | tee "$artifact_dir/stage7b-b-service-release.txt"
cargo test -p runtime-durable-service --test stage7b_writer_lock_subprocess -- --nocapture 2>&1 \
  | tee "$artifact_dir/writer-lock-subprocess.txt"
cargo test --release -p runtime-durable-service --test stage7b_writer_lock_subprocess -- --nocapture 2>&1 \
  | tee "$artifact_dir/writer-lock-subprocess-release.txt"
cargo test -p strategy-runtime-core --lib stage6 --no-fail-fast 2>&1 \
  | tee "$artifact_dir/stage6-regression.txt"
cargo test --workspace --all-targets 2>&1 | tee "$artifact_dir/workspace-tests.txt"
cargo test --workspace --doc 2>&1 | tee "$artifact_dir/workspace-docs.txt"
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 \
  | tee "$artifact_dir/clippy.txt"
rustc --version | tee "$artifact_dir/toolchain.txt"
cargo --version | tee -a "$artifact_dir/toolchain.txt"
echo "stage7b-b-gate: PASS artifact_dir=$artifact_dir"
