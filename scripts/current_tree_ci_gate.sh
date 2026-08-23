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

accepted_evidence="$artifact_dir/accepted-stage8a5-evidence"
if ! STAGE8A5_ARTIFACT_DIR="$accepted_evidence" \
  bash "$replay_root/repo/scripts/stage8a5_gate.sh"; then
  echo "current-tree-ci-gate: accepted Stage 8A5 replay failed; nested diagnostics follow" >&2
  failure_files=0
  while IFS= read -r -d '' failure_file; do
    if grep -Eq ' \.\.\. FAILED$|panicked at|error: test failed|gate: FAIL|: FAIL ' \
      "$failure_file"; then
      echo "===== ${failure_file#"$artifact_dir/"} =====" >&2
      tail -n 200 "$failure_file" >&2
      failure_files=$((failure_files + 1))
      if [[ "$failure_files" -ge 20 ]]; then
        echo "current-tree-ci-gate: nested diagnostic file limit reached" >&2
        break
      fi
    fi
  done < <(find "$accepted_evidence" -type f -name '*.txt' -print0 2>/dev/null)
  if [[ "$failure_files" -eq 0 ]]; then
    echo "current-tree-ci-gate: no standard failure signature; bounded gate tails follow" >&2
    python3 - "$artifact_dir" <<'PY' >&2
import sys
from pathlib import Path

root = Path(sys.argv[1])
files = sorted(
    (path for path in root.rglob("*") if path.is_file()),
    key=lambda path: path.stat().st_mtime_ns,
    reverse=True,
)
for path in files[:30]:
    print(f"===== {path.relative_to(root)} =====")
    data = path.read_bytes()[-16_384:]
    print(data.decode("utf-8", errors="replace"))
print(f"current-tree-ci-gate: diagnostic_files_available={len(files)} emitted={min(30, len(files))}")
PY
  fi
  echo "current-tree-ci-gate: nested_failure_files=$failure_files" >&2
  exit 1
fi
rm -rf "$replay_root"

echo "current-tree-ci-gate: PASS source_ref=$source_ref accepted_stage8a5_ref=$accepted_stage8a5_ref"
