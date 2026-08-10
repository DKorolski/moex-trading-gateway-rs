#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
artifact_dir="${STAGE6A_R1_ARTIFACT_DIR:-${TMPDIR:-/tmp}/stage6a-r1-artifact.$$}"
mkdir -p "$artifact_dir"

cargo fmt --all -- --check
python3 scripts/stage6a_check.py
python3 scripts/stage6a_negative_harness.py | tee "$artifact_dir/stage6a-r1-negative.txt"
python3 scripts/stage6a_closed_surface_check.py

if [[ "${STAGE6A_R1_SKIP_DETACHED_GATES:-0}" != "1" ]]; then
  predecessor="$artifact_dir/detached-stage6a-94c3fd9"
  git clone --quiet --no-hardlinks --shared . "$predecessor"
  git -C "$predecessor" checkout --quiet -B stage6-durable-chain 94c3fd9d841db5b207ad7fd50090e64c2770365d
  STAGE6A_ARTIFACT_DIR="$artifact_dir/detached-stage6a-artifacts" \
    bash "$predecessor/scripts/stage6a_gate.sh"
  rm -rf "$predecessor"
fi

cargo test -p strategy-runtime-core --lib stage6a_
cargo test --release -p strategy-runtime-core --lib stage6a_
cargo test --workspace --all-targets
cargo test --workspace --doc
cargo clippy --workspace --all-targets --all-features -- -D warnings

if [[ "${STAGE6A_R1_SKIP_PRESEAL:-0}" != "1" ]]; then python3 scripts/stage6a_preseal_check.py; fi
rustc --version | tee "$artifact_dir/toolchain.txt"
cargo --version | tee -a "$artifact_dir/toolchain.txt"
echo "stage6a-r1-gate: PASS artifact_dir=$artifact_dir"
