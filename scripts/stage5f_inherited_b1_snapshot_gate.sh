#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python_bin="${PYTHON:-python3}"
readonly accepted_b1_ref="86b43c448fb65a3c54b6118d04d3f40e08e74ad7"

if ! git -C "$repo_root" cat-file -e "${accepted_b1_ref}^{commit}"; then
  echo "stage5f-inherited-b1-snapshot-gate: accepted B1 commit is unavailable" >&2
  exit 1
fi

if ! git -C "$repo_root" merge-base --is-ancestor "$accepted_b1_ref" HEAD; then
  echo "stage5f-inherited-b1-snapshot-gate: accepted B1 is not an ancestor" >&2
  exit 1
fi

snapshot_root="$(mktemp -d "${TMPDIR:-/tmp}/stage5f-inherited-b1.XXXXXX")"
cleanup() {
  rm -rf "$snapshot_root"
}
trap cleanup EXIT HUP INT TERM

git -C "$repo_root" archive --format=tar "$accepted_b1_ref" \
  | tar -xf - -C "$snapshot_root"

(
  cd "$snapshot_root"
  bash scripts/forbidden_surface_scan.sh
  "$python_bin" scripts/stage5f_atomic_hybrid_semantics_negative_harness.py
  "$python_bin" scripts/stage5f_ci_snapshot_inheritance_negative_harness.py
)

echo "stage5f-inherited-b1-snapshot-gate: ok source_ref=$accepted_b1_ref negative_cases=30"
