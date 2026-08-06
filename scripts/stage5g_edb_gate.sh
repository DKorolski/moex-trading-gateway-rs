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

r6_ref="4ece2c7c83ca5575dbca306b5fa29a48dae2bd47"
snapshot_parent="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-edb-predecessor.XXXXXX")"
snapshot_root="$snapshot_parent/worktree"
cleanup() {
  if [[ -e "$snapshot_root/.git" ]]; then
    git worktree remove --force "$snapshot_root" >/dev/null 2>&1 || true
  fi
  rm -rf "$snapshot_parent"
}
trap cleanup EXIT
git worktree add --detach "$snapshot_root" "$r6_ref" >/dev/null
(
  cd "$snapshot_root"
  bash scripts/stage5g_eda_r6_gate.sh
)

echo "stage5g-e-d-b-r1-gate: PASS"
