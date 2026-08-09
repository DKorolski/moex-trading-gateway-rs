#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
artifact_dir="${STAGE6A_ARTIFACT_DIR:-${TMPDIR:-/tmp}/stage6a-artifact.$$}"
mkdir -p "$artifact_dir"

cargo fmt --all -- --check
python3 scripts/stage6a_check.py
python3 scripts/stage6a_negative_harness.py | tee "$artifact_dir/stage6a-negative.txt"
python3 scripts/stage6a_closed_surface_check.py

if [[ "${STAGE6A_SKIP_DETACHED_GATES:-0}" != "1" ]]; then
  transition="$artifact_dir/detached-transition"
  git clone --quiet --no-hardlinks --shared . "$transition"
  git -C "$transition" checkout --quiet -B stage5g-lifecycle 14359aadb3178c83692441b748b060d06ce12903
  TRANSITION_5_TO_6_SKIP_DETACHED_STAGE5=1 TRANSITION_5_TO_6_SKIP_PRESEAL=1 \
    bash "$transition/scripts/transition_gate_5_to_6.sh"
  rm -rf "$transition"

  stage5="$artifact_dir/detached-stage5g-h"
  git clone --quiet --no-hardlinks --shared . "$stage5"
  git -C "$stage5" checkout --quiet -B stage5g-lifecycle 013e63bbee57c4f2d00a0587e9343ab623efba0d
  STAGE5G_H_SKIP_DETACHED_G_GATE=1 STAGE5G_H_SKIP_PRESEAL=1 bash "$stage5/scripts/stage5g_h_gate.sh"
  rm -rf "$stage5"
fi

cargo test -p strategy-runtime-core --lib stage6a_
cargo test --release -p strategy-runtime-core --lib stage6a_
cargo test --workspace --all-targets
cargo test --workspace --doc
cargo clippy --workspace --all-targets --all-features -- -D warnings

if [[ "${STAGE6A_SKIP_PRESEAL:-0}" != "1" ]]; then python3 scripts/stage6a_preseal_check.py; fi
rustc --version | tee "$artifact_dir/toolchain.txt"
cargo --version | tee -a "$artifact_dir/toolchain.txt"
echo "stage6a-gate: PASS artifact_dir=$artifact_dir"
