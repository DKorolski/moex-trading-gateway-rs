#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5g_eda_r1_check.py
python3 scripts/stage5g_eda_r1_negative_harness.py
cargo fmt --all -- --check
cargo test -p strategy-runtime-core --lib stage5g_fresh_broker_truth
cargo test --release -p strategy-runtime-core --lib stage5g_fresh_broker_truth
cargo test -p strategy-runtime-core --lib
cargo clippy -p strategy-runtime-core --all-targets --all-features -- -D warnings

rejected_eda_ref="f44b154753ea8b60a73cfb6ee3b5e487263dcb3b"
snapshot_parent="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-eda-r1-predecessor.XXXXXX")"
snapshot_root="$snapshot_parent/worktree"
cleanup() {
  if [[ -e "$snapshot_root/.git" ]]; then
    git worktree remove --force "$snapshot_root" >/dev/null 2>&1 || true
  fi
  rm -rf "$snapshot_parent"
}
trap cleanup EXIT
git worktree add --detach "$snapshot_root" "$rejected_eda_ref" >/dev/null
(
  cd "$snapshot_root"
  bash scripts/stage5g_ed_gate.sh
)

echo "stage5g-e-d-a-r1-gate: PASS"
