#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5g_edc_check.py
python3 scripts/stage5g_edc_negative_harness.py
python3 scripts/stage5g_edc_preseal_check.py
cargo fmt --all -- --check
cargo test -p strategy-runtime-core --lib stage5g_edc_
cargo test --release -p strategy-runtime-core --lib stage5g_edc_
cargo test -p strategy-runtime-core --lib
cargo test -p strategy-runtime-core --doc
cargo clippy -p strategy-runtime-core --all-targets --all-features -- -D warnings

accepted_ref="2b2bcc671c68722b3b84b914b785ffcb83f6802d"
forbidden_ref="bd4742ef4b727ae8fa43d561c6674dea71b86b57"
snapshot_parent="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-edc-predecessor.XXXXXX")"
snapshot_root="$snapshot_parent/worktree"
forbidden_root="$snapshot_parent/forbidden-worktree"
cleanup() {
  if [[ -e "$snapshot_root/.git" ]]; then
    git worktree remove --force "$snapshot_root" >/dev/null 2>&1 || true
  fi
  if [[ -e "$forbidden_root/.git" ]]; then
    git worktree remove --force "$forbidden_root" >/dev/null 2>&1 || true
  fi
  rm -rf "$snapshot_parent"
}
trap cleanup EXIT

# The portable scanner is an immutable Stage 5F authority whose source-set
# baseline intentionally rejects later accepted Stage 5G files. Run that
# scanner and its mutation matrix against its exact accepted tree; the current
# e-d-c closed surface is enforced above by stage5g_edc_check/negative_harness.
git worktree add --detach "$forbidden_root" "$forbidden_ref" >/dev/null
(
  cd "$forbidden_root"
  bash scripts/forbidden_surface_scan.sh
  bash scripts/forbidden_surface_negative_harness.sh
)
git worktree remove --force "$forbidden_root" >/dev/null

git worktree add --detach "$snapshot_root" "$accepted_ref" >/dev/null
(
  cd "$snapshot_root"
  bash scripts/stage5g_edb_r5_gate.sh
)

echo "stage5g-edc-gate: PASS"
