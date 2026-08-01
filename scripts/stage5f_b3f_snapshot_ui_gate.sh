#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly accepted_b3f_ref="e14654f7129aa61011931306140a3bfefe2fcfbc"
snapshot_root=""

cleanup() {
  local status=$?
  if [[ -n "$snapshot_root" ]]; then
    rm -rf "$snapshot_root"
  fi
  exit "$status"
}
trap cleanup EXIT HUP INT TERM

git -C "$repo_root" cat-file -e "${accepted_b3f_ref}^{commit}"
snapshot_root="$(mktemp -d "${TMPDIR:-/tmp}/stage5f-b3f-ui.XXXXXX")"
git -C "$repo_root" archive "$accepted_b3f_ref" | tar -x -C "$snapshot_root"

(
  cd "$snapshot_root"
  python3 scripts/stage5e_b3f_production_ui_harness.py
)

echo "stage5f-b3f-snapshot-ui-gate: ok source_ref=$accepted_b3f_ref cases=8"
