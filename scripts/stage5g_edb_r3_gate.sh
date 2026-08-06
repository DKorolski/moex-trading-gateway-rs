#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5g_edb_check.py
python3 scripts/stage5g_edb_negative_harness.py
python3 scripts/stage5g_edb_preseal_check.py
cargo fmt --all -- --check
cargo test -p strategy-runtime-core --lib stage5g_edb
cargo test --release -p strategy-runtime-core --lib stage5g_edb
cargo test -p strategy-runtime-core --lib
cargo clippy -p strategy-runtime-core --all-targets --all-features -- -D warnings

r2_ref="c5f84bbcf7c1b44c1eac9c2e99857834d333a4c4"
snapshot_parent="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-edb-r3-predecessor.XXXXXX")"
snapshot_root="$snapshot_parent/worktree"
cleanup() {
  if [[ -e "$snapshot_root/.git" ]]; then
    git worktree remove --force "$snapshot_root" >/dev/null 2>&1 || true
  fi
  rm -rf "$snapshot_parent"
}
trap cleanup EXIT
git worktree add --detach "$snapshot_root" "$r2_ref" >/dev/null
(
  cd "$snapshot_root"
  bash scripts/stage5g_edb_r2_gate.sh
)

echo "stage5g-e-d-b-r3-gate: PASS"
