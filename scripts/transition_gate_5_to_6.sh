#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
artifact_dir="${TRANSITION_5_TO_6_ARTIFACT_DIR:-${TMPDIR:-/tmp}/transition-5-to-6.$$}"
mkdir -p "$artifact_dir"

cargo fmt --all -- --check
python3 scripts/transition_gate_5_to_6_check.py
python3 scripts/transition_gate_5_to_6_negative_harness.py
python3 scripts/transition_gate_5_to_6_closed_surface_check.py

if [[ "${TRANSITION_5_TO_6_SKIP_DETACHED_STAGE5:-0}" != "1" ]]; then
  detached="$artifact_dir/detached-stage5-closure"
  git clone --quiet --no-hardlinks --shared . "$detached"
  git -C "$detached" checkout --quiet -B stage5g-lifecycle 013e63bbee57c4f2d00a0587e9343ab623efba0d
  STAGE5G_H_ARTIFACT_DIR="$artifact_dir/detached-stage5-artifacts" bash "$detached/scripts/stage5g_h_gate.sh"
  rm -rf "$detached"
fi

cargo test --workspace --all-targets
cargo test --workspace --doc
cargo clippy --workspace --all-targets --all-features -- -D warnings

if [[ "${TRANSITION_5_TO_6_SKIP_PRESEAL:-0}" != "1" ]]; then
  python3 scripts/transition_gate_5_to_6_preseal_check.py
fi
echo "transition-gate-5-to-6: PASS artifact_dir=$artifact_dir"
