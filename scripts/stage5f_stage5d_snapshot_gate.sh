#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly accepted_stage5d_ref="9ebbfd29d0346be5149dac746225866f0c8d0257"
mode="${1:-}"
snapshot_root=""

cleanup() {
  local status=$?
  if [[ -n "$snapshot_root" ]]; then
    rm -rf "$snapshot_root"
  fi
  exit "$status"
}
trap cleanup EXIT HUP INT TERM

if [[ "$mode" != "check" && "$mode" != "negative" ]]; then
  echo "usage: $0 check|negative" >&2
  exit 2
fi

git -C "$repo_root" cat-file -e "${accepted_stage5d_ref}^{commit}"
snapshot_root="$(mktemp -d "${TMPDIR:-/tmp}/stage5f-stage5d.XXXXXX")"
git -C "$repo_root" archive "$accepted_stage5d_ref" | tar -x -C "$snapshot_root"

(
  cd "$snapshot_root"
  if [[ "$mode" == "check" ]]; then
    python3 scripts/stage5d_additive_freeze_check.py
  else
    python3 scripts/stage5d_additive_freeze_negative_harness.py
  fi
)

echo "stage5f-stage5d-snapshot-gate: ok mode=$mode source_ref=$accepted_stage5d_ref"
