#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
artifact_dir="${STAGE6B_R1_ARTIFACT_DIR:-${TMPDIR:-/tmp}/stage6b-r1-artifact.$$}"
mkdir -p "$artifact_dir"

cargo fmt --all -- --check
python3 scripts/stage6b_check.py
python3 scripts/stage6b_negative_harness.py | tee "$artifact_dir/stage6b-r1-negative.txt"
python3 scripts/stage6b_closed_surface_check.py

if [[ "${STAGE6B_R1_SKIP_DETACHED_GATES:-0}" != "1" ]]; then
  predecessor="$artifact_dir/detached-stage6b-6dbc4e0"
  git clone --quiet --no-hardlinks --shared . "$predecessor"
  git -C "$predecessor" checkout --quiet -B stage6-durable-chain 6dbc4e021f61860069c599ccd526a83e4bca01a6
  STAGE6B_ARTIFACT_DIR="$artifact_dir/detached-stage6b-artifacts" \
    bash "$predecessor/scripts/stage6b_gate.sh"
  rm -rf "$predecessor"
fi

cargo test -p strategy-runtime-core --lib stage6b_
cargo test --release -p strategy-runtime-core --lib stage6b_
cargo test --workspace --all-targets
cargo test --workspace --doc
cargo clippy --workspace --all-targets --all-features -- -D warnings

if [[ "${STAGE6B_R1_SKIP_PRESEAL:-0}" != "1" ]]; then
  python3 scripts/stage6b_preseal_check.py
fi
rustc --version | tee "$artifact_dir/toolchain.txt"
cargo --version | tee -a "$artifact_dir/toolchain.txt"
echo "stage6b-r1-gate: PASS artifact_dir=$artifact_dir"
