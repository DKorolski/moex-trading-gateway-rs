#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
artifact_dir="${STAGE6E_R1_ARTIFACT_DIR:-${TMPDIR:-/tmp}/stage6e-r1-artifact.$$}"
mkdir -p "$artifact_dir"

cargo fmt --all -- --check
python3 scripts/stage6e_r1_check.py
python3 scripts/stage6e_r1_negative_harness.py | tee "$artifact_dir/stage6e-r1-negative.txt"
python3 scripts/stage6e_r1_closed_surface_check.py

if [[ "${STAGE6E_R1_SKIP_DETACHED_GATE:-0}" != "1" ]]; then
  accepted="$artifact_dir/detached-stage6e-ec71791"
  git clone --quiet --no-hardlinks --shared . "$accepted"
  git -C "$accepted" checkout --quiet -B stage6-durable-chain ec71791563a933889eb825f6f8f0846915ba6415
  STAGE6E_SKIP_PRESEAL=1 \
    STAGE6E_ARTIFACT_DIR="$artifact_dir/detached-stage6e-artifacts" \
    bash "$accepted/scripts/stage6e_gate.sh"
  rm -rf "$accepted"
fi

cargo test -p strategy-runtime-core --lib stage6e_r1_ --no-fail-fast
cargo test --release -p strategy-runtime-core --lib stage6e_r1_ --no-fail-fast
cargo test -p strategy-runtime-core --lib stage6e_ --no-fail-fast
cargo test --release -p strategy-runtime-core --lib stage6e_ --no-fail-fast
cargo test -p strategy-runtime-core --lib stage6d_ --no-fail-fast
cargo test -p strategy-runtime-core --lib stage6a_
cargo test -p strategy-runtime-core --lib stage6b_
cargo test -p strategy-runtime-core --lib stage6c_
cargo test --workspace --all-targets
cargo test --workspace --doc
cargo clippy --workspace --all-targets --all-features -- -D warnings

if [[ "${STAGE6E_R1_SKIP_PRESEAL:-0}" != "1" ]]; then
  python3 scripts/stage6e_r1_preseal_check.py
fi
rustc --version | tee "$artifact_dir/toolchain.txt"
cargo --version | tee -a "$artifact_dir/toolchain.txt"
echo "stage6e-r1-gate: PASS artifact_dir=$artifact_dir"
