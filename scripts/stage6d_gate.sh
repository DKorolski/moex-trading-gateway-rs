#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
artifact_dir="${STAGE6D_ARTIFACT_DIR:-${TMPDIR:-/tmp}/stage6d-artifact.$$}"
mkdir -p "$artifact_dir"

cargo fmt --all -- --check
python3 scripts/stage6d_check.py
python3 scripts/stage6d_negative_harness.py | tee "$artifact_dir/stage6d-negative.txt"
python3 scripts/stage6d_closed_surface_check.py

if [[ "${STAGE6D_SKIP_DETACHED_GATES:-0}" != "1" ]]; then
  stage6c="$artifact_dir/detached-stage6c-r1-e10d8fb"
  git clone --quiet --no-hardlinks --shared . "$stage6c"
  git -C "$stage6c" checkout --quiet -B stage6-durable-chain e10d8fb0f9e095a849b1e56779a0597606d22111
  STAGE6C_R1_SKIP_DETACHED_GATES=1 STAGE6C_R1_SKIP_PRESEAL=1 \
    STAGE6C_R1_ARTIFACT_DIR="$artifact_dir/detached-stage6c-r1-artifacts" \
    bash "$stage6c/scripts/stage6c_r1_gate.sh"
  rm -rf "$stage6c"

  stage6b="$artifact_dir/detached-stage6b-r1-f0d5e39"
  git clone --quiet --no-hardlinks --shared . "$stage6b"
  git -C "$stage6b" checkout --quiet -B stage6-durable-chain f0d5e3912243ba85c6f372722c97e815f254a962
  STAGE6B_R1_SKIP_DETACHED_GATES=1 STAGE6B_R1_SKIP_PRESEAL=1 \
    STAGE6B_R1_ARTIFACT_DIR="$artifact_dir/detached-stage6b-r1-artifacts" \
    bash "$stage6b/scripts/stage6b_r1_gate.sh"
  rm -rf "$stage6b"
fi

cargo test -p strategy-runtime-core --lib stage6d_ --no-fail-fast
cargo test --release -p strategy-runtime-core --lib stage6d_ --no-fail-fast
cargo test -p strategy-runtime-core --lib stage6a_
cargo test -p strategy-runtime-core --lib stage6b_
cargo test -p strategy-runtime-core --lib stage6c_
cargo test --workspace --all-targets
cargo test --workspace --doc
cargo clippy --workspace --all-targets --all-features -- -D warnings

if [[ "${STAGE6D_SKIP_PRESEAL:-0}" != "1" ]]; then
  python3 scripts/stage6d_preseal_check.py
fi
rustc --version | tee "$artifact_dir/toolchain.txt"
cargo --version | tee -a "$artifact_dir/toolchain.txt"
echo "stage6d-gate: PASS artifact_dir=$artifact_dir"
