#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
artifact_dir="${STAGE5G_H_ARTIFACT_DIR:-${TMPDIR:-/tmp}/stage5g-h-artifact.$$}"
mkdir -p "$artifact_dir"
debug="$artifact_dir/stage5g-h-sequential.debug.json"
release="$artifact_dir/stage5g-h-sequential.release.json"
parallel="$artifact_dir/stage5g-h-true-parallel.json"

cargo fmt --all -- --check
python3 scripts/stage5g_h_check.py
python3 scripts/stage5g_h_negative_harness.py
python3 scripts/stage5g_h_closed_surface_check.py

if [[ "${STAGE5G_H_SKIP_DETACHED_G_GATE:-0}" != "1" ]]; then
  detached="$artifact_dir/detached-stage5g-g"
  git clone --quiet --no-hardlinks --shared . "$detached"
  git -C "$detached" checkout --quiet ee0505dfee71f043f3185c16cbdd563e3b36a6c1
  STAGE5G_G_ARTIFACT_DIR="$artifact_dir/detached-g-artifacts" bash "$detached/scripts/stage5g_g_gate.sh"
  rm -rf "$detached"
fi

cargo run -q -p strategy-runtime-core --features stage5g-artifact-fixtures --bin stage5g_g_lifecycle_artifact -- --sequential > "$debug"
cargo run -q --release -p strategy-runtime-core --features stage5g-artifact-fixtures --bin stage5g_g_lifecycle_artifact -- --sequential > "$release"
cargo run -q -p strategy-runtime-core --features stage5g-artifact-fixtures --bin stage5g_g_lifecycle_artifact > "$parallel"
cmp docs/stage-5/accepted-stage5g-g-lifecycle-artifact.json "$debug"
cmp "$debug" "$release"
cmp "$debug" "$parallel"
python3 scripts/stage5g_h_check.py --artifact "$debug" --parallel-artifact "$parallel"

cargo test --workspace --all-targets
cargo test --workspace --doc
cargo clippy --workspace --all-targets --all-features -- -D warnings

if [[ "${STAGE5G_H_SKIP_PRESEAL:-0}" != "1" ]]; then
  python3 scripts/stage5g_h_preseal_check.py
fi
shasum -a 256 "$debug"
echo "stage5g-h-gate: PASS artifact_dir=$artifact_dir"
