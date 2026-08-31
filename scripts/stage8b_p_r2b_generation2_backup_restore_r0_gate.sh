#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_p_r2b_trust_rebind_r0_check.py
python3 scripts/stage8b_p_r2b_trust_rebind_r0_negative_harness.py
python3 scripts/stage8b_p_r2b_generation2_backup_restore_r0_check.py
python3 scripts/stage8b_p_r2b_generation2_backup_restore_r0_negative_harness.py

python3 -m py_compile \
  scripts/stage8b_p_r2b_generation2_backup_identity.py \
  scripts/stage8b_p_r2b_generation2_backup_restore_r0_operate.py \
  scripts/stage8b_p_r2b_generation2_backup_restore_r0_check.py \
  scripts/stage8b_p_r2b_generation2_backup_restore_r0_negative_harness.py \
  scripts/stage8b_p_r2b_generation2_backup_restore_r0_handoff_safety_check.py \
  scripts/stage8b_p_r2b_generation2_backup_restore_r0_handoff_negative_harness.py \
  scripts/make_stage8b_p_r2b_generation2_backup_restore_r0_handoff.py

for document in \
  docs/stage-8/stage8b-p-r2b-generation2-backup-restore-r0-authority.json \
  docs/stage-8/stage8b-p-r2b-generation2-backup-restore-r0-receipt.json \
  docs/stage-8/stage8b-p-r2b-generation2-restore-destruction-r0-receipt.json; do
  python3 -m json.tool "$document" >/dev/null
done

cargo fmt --manifest-path tools/stage8b-readonly-preflight/Cargo.toml -- --check
cargo test --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets
cargo clippy --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets -- -D warnings
git diff --check

echo "stage8b-generation2-backup-restore-r0-gate: PASS generation=2 rust_tests=55 negative=42 backup=VERIFIED restore_deleted=true ciphertext_in_git=false private_key_in_git=false active=false authorization=NOT_ISSUED finam=false"
