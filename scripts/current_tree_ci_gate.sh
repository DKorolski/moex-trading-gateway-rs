#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

source_ref="unavailable-no-git"
if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  source_ref="$(git rev-parse HEAD)"
fi
echo "current-tree-ci-gate: source_ref=$source_ref"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py

accepted_stage8a5_ref="bf58b47fdef8af774a4107455dfcc6204e594283"
accepted_stage8a5_gate_sha256="1361ad49d41351484cf61c86822deb640818e755b7b35bda44592fd437ff69f8"
artifact_dir="${CURRENT_TREE_CI_ARTIFACT_DIR:-$repo_root/tmp/current-tree-ci}"
if [[ "$artifact_dir" != /* ]]; then
  artifact_dir="$repo_root/$artifact_dir"
fi
replay_root="$artifact_dir/accepted-stage8a5-replay"
rm -rf "$replay_root"
mkdir -p "$replay_root"

git cat-file -e "${accepted_stage8a5_ref}^{commit}"
git clone --quiet --no-hardlinks --shared "$repo_root" "$replay_root/repo"
git -C "$replay_root/repo" checkout --quiet -B stage8a5-aggregate-acceptance \
  "$accepted_stage8a5_ref"
actual_gate_sha256="$(shasum -a 256 "$replay_root/repo/scripts/stage8a5_gate.sh" | awk '{print $1}')"
test "$actual_gate_sha256" = "$accepted_stage8a5_gate_sha256"

STAGE8A5_ARTIFACT_DIR="$artifact_dir/accepted-stage8a5-evidence" \
  bash "$replay_root/repo/scripts/stage8a5_gate.sh"
rm -rf "$replay_root"

echo "current-tree-ci-gate: PASS source_ref=$source_ref accepted_stage8a5_ref=$accepted_stage8a5_ref"
