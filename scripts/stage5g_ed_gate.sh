#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5g_ed_check.py
python3 scripts/stage5g_ed_negative_harness.py
cargo fmt --all -- --check
cargo test -p strategy-runtime-core stage5g_fresh_broker_truth --lib
cargo clippy -p strategy-runtime-core --all-targets --all-features -- -D warnings

accepted_ec_ref="b9db87947723cf9c50e64b5fcc3b5ab30e857fd1"
snapshot_parent="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-ed-ec-predecessor.XXXXXX")"
snapshot_root="$snapshot_parent/worktree"
cleanup() {
  if [[ -e "$snapshot_root/.git" ]]; then
    git worktree remove --force "$snapshot_root" >/dev/null 2>&1 || true
  fi
  rm -rf "$snapshot_parent"
}
trap cleanup EXIT
git worktree add --detach "$snapshot_root" "$accepted_ec_ref" >/dev/null
(
  cd "$snapshot_root"
  bash scripts/stage5g_ec_gate.sh
)

echo "stage5g-ed-a-gate: PASS"
