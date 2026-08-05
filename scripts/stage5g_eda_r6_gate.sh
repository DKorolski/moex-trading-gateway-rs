#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5g_eda_r6_check.py
python3 scripts/stage5g_eda_r6_negative_harness.py
python3 scripts/stage5g_eda_r6_preseal_check.py
cargo fmt --all -- --check
cargo test -p strategy-runtime-core --lib stage5g_fresh_broker_truth
cargo test --release -p strategy-runtime-core --lib stage5g_fresh_broker_truth
cargo test -p strategy-runtime-core --lib
cargo clippy -p strategy-runtime-core --all-targets --all-features -- -D warnings

r5_ref="c84ee07c2700f04b5c070eab713598777d5195b6"
snapshot_parent="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-eda-r6-predecessor.XXXXXX")"
snapshot_root="$snapshot_parent/worktree"
cleanup() {
  if [[ -e "$snapshot_root/.git" ]]; then
    git worktree remove --force "$snapshot_root" >/dev/null 2>&1 || true
  fi
  rm -rf "$snapshot_parent"
}
trap cleanup EXIT
git worktree add --detach "$snapshot_root" "$r5_ref" >/dev/null
(
  cd "$snapshot_root"
  bash scripts/stage5g_eda_r5_gate.sh
)

echo "stage5g-e-d-a-r6-gate: PASS"
