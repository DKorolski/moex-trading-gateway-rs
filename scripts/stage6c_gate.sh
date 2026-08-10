#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
artifact_dir="${STAGE6C_ARTIFACT_DIR:-${TMPDIR:-/tmp}/stage6c-artifact.$$}"
mkdir -p "$artifact_dir"

cargo fmt --all -- --check
python3 scripts/stage6c_check.py
python3 scripts/stage6c_negative_harness.py | tee "$artifact_dir/stage6c-negative.txt"
python3 scripts/stage6c_closed_surface_check.py

if [[ "${STAGE6C_SKIP_DETACHED_GATES:-0}" != "1" ]]; then
  stage6b="$artifact_dir/detached-stage6b-r1-f0d5e39"
  git clone --quiet --no-hardlinks --shared . "$stage6b"
  git -C "$stage6b" checkout --quiet -B stage6-durable-chain f0d5e3912243ba85c6f372722c97e815f254a962
  STAGE6B_R1_ARTIFACT_DIR="$artifact_dir/detached-stage6b-r1-artifacts" \
    bash "$stage6b/scripts/stage6b_r1_gate.sh"
  rm -rf "$stage6b"

  stage6a="$artifact_dir/detached-stage6a-r2-c399e2b"
  git clone --quiet --no-hardlinks --shared . "$stage6a"
  git -C "$stage6a" checkout --quiet -B stage6-durable-chain c399e2bc2c7e62cc2116a6eac970058bb47c4a49
  STAGE6A_R2_ARTIFACT_DIR="$artifact_dir/detached-stage6a-r2-artifacts" \
    bash "$stage6a/scripts/stage6a_r2_gate.sh"
  rm -rf "$stage6a"
fi

cargo test -p strategy-runtime-core --lib stage6c_
cargo test --release -p strategy-runtime-core --lib stage6c_
cargo test -p strategy-runtime-core --lib stage6a_
cargo test -p strategy-runtime-core --lib stage6b_
cargo test --workspace --all-targets
cargo test --workspace --doc
cargo clippy --workspace --all-targets --all-features -- -D warnings

if [[ "${STAGE6C_SKIP_PRESEAL:-0}" != "1" ]]; then
  python3 scripts/stage6c_preseal_check.py
fi
rustc --version | tee "$artifact_dir/toolchain.txt"
cargo --version | tee -a "$artifact_dir/toolchain.txt"
echo "stage6c-gate: PASS artifact_dir=$artifact_dir"
