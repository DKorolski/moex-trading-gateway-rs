#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
artifact_dir="${STAGE6B_ARTIFACT_DIR:-${TMPDIR:-/tmp}/stage6b-artifact.$$}"
mkdir -p "$artifact_dir"

cargo fmt --all -- --check
python3 scripts/stage6b_check.py
python3 scripts/stage6b_negative_harness.py | tee "$artifact_dir/stage6b-negative.txt"
python3 scripts/stage6b_closed_surface_check.py

if [[ "${STAGE6B_SKIP_DETACHED_GATES:-0}" != "1" ]]; then
  predecessor="$artifact_dir/detached-stage6a-c399e2b"
  git clone --quiet --no-hardlinks --shared . "$predecessor"
  git -C "$predecessor" checkout --quiet -B stage6-durable-chain c399e2bc2c7e62cc2116a6eac970058bb47c4a49
  STAGE6A_R2_ARTIFACT_DIR="$artifact_dir/detached-stage6a-r2-artifacts" \
    bash "$predecessor/scripts/stage6a_r2_gate.sh"
  rm -rf "$predecessor"
fi

cargo test -p strategy-runtime-core --lib stage6b_
cargo test --release -p strategy-runtime-core --lib stage6b_
cargo test --workspace --all-targets
cargo test --workspace --doc
cargo clippy --workspace --all-targets --all-features -- -D warnings

if [[ "${STAGE6B_SKIP_PRESEAL:-0}" != "1" ]]; then python3 scripts/stage6b_preseal_check.py; fi
rustc --version | tee "$artifact_dir/toolchain.txt"
cargo --version | tee -a "$artifact_dir/toolchain.txt"
echo "stage6b-gate: PASS artifact_dir=$artifact_dir"
