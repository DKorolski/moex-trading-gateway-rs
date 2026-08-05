#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5g_eda_r5_check.py
python3 scripts/stage5g_eda_r5_negative_harness.py
python3 scripts/stage5g_eda_r5_preseal_check.py
cargo fmt --all -- --check
cargo test -p strategy-runtime-core --lib stage5g_fresh_broker_truth
cargo test --release -p strategy-runtime-core --lib stage5g_fresh_broker_truth
cargo test -p strategy-runtime-core --lib
cargo clippy -p strategy-runtime-core --all-targets --all-features -- -D warnings

r4_ref="49357a2d49d45ab6f5f9cb8b3f0e11dfb6b97c30"
snapshot_parent="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-eda-r5-predecessor.XXXXXX")"
snapshot_root="$snapshot_parent/worktree"
cleanup() {
  if [[ -e "$snapshot_root/.git" ]]; then
    git worktree remove --force "$snapshot_root" >/dev/null 2>&1 || true
  fi
  rm -rf "$snapshot_parent"
}
trap cleanup EXIT
git worktree add --detach "$snapshot_root" "$r4_ref" >/dev/null
(
  cd "$snapshot_root"
  bash scripts/stage5g_eda_r4_gate.sh
)

echo "stage5g-e-d-a-r5-gate: PASS"
