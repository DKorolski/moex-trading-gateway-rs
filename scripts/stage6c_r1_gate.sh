#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
artifact_dir="${STAGE6C_R1_ARTIFACT_DIR:-${TMPDIR:-/tmp}/stage6c-r1-artifact.$$}"
mkdir -p "$artifact_dir"

pin_closed_lineage_refs() {
  local repository="$1"
  local closed_ref="14359aadb3178c83692441b748b060d06ce12903"
  git -C "$repository" update-ref refs/heads/main "$closed_ref"
  git -C "$repository" update-ref refs/heads/stage5g-lifecycle "$closed_ref"
  git -C "$repository" update-ref refs/remotes/origin/main "$closed_ref"
  git -C "$repository" update-ref refs/remotes/origin/stage5g-lifecycle "$closed_ref"
}

cargo fmt --all -- --check
python3 scripts/stage6c_check.py
python3 scripts/stage6c_negative_harness.py | tee "$artifact_dir/stage6c-r1-negative.txt"
python3 scripts/stage6c_closed_surface_check.py

if [[ "${STAGE6C_R1_SKIP_DETACHED_GATES:-0}" != "1" ]]; then
  stage6c="$artifact_dir/detached-stage6c-a4e55c4"
  git clone --quiet --no-hardlinks . "$stage6c"
  git -C "$stage6c" checkout --quiet -B stage6-durable-chain a4e55c42aac6d2470d6ab874c61c19be1b771b3f
  pin_closed_lineage_refs "$stage6c"
  STAGE6C_ARTIFACT_DIR="$artifact_dir/detached-stage6c-artifacts" \
    bash "$stage6c/scripts/stage6c_gate.sh"
  rm -rf "$stage6c"

  stage6b="$artifact_dir/detached-stage6b-r1-f0d5e39"
  git clone --quiet --no-hardlinks . "$stage6b"
  git -C "$stage6b" checkout --quiet -B stage6-durable-chain f0d5e3912243ba85c6f372722c97e815f254a962
  pin_closed_lineage_refs "$stage6b"
  STAGE6B_R1_ARTIFACT_DIR="$artifact_dir/detached-stage6b-r1-artifacts" \
    bash "$stage6b/scripts/stage6b_r1_gate.sh"
  rm -rf "$stage6b"
fi

cargo test -p strategy-runtime-core --lib stage6c_
cargo test --release -p strategy-runtime-core --lib stage6c_
cargo test -p strategy-runtime-core --lib stage6a_
cargo test -p strategy-runtime-core --lib stage6b_
cargo test --workspace --all-targets
cargo test --workspace --doc
cargo clippy --workspace --all-targets --all-features -- -D warnings

if [[ "${STAGE6C_R1_SKIP_PRESEAL:-0}" != "1" ]]; then
  python3 scripts/stage6c_preseal_check.py
fi
rustc --version | tee "$artifact_dir/toolchain.txt"
cargo --version | tee -a "$artifact_dir/toolchain.txt"
echo "stage6c-r1-gate: PASS artifact_dir=$artifact_dir"
