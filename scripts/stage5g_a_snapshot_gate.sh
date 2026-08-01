#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python_bin="${PYTHON:-python3}"
readonly accepted_stage5g_a_ref="011fd4b7baaa41fffdad7d3c28e463b7977f5989"

if ! git -C "$repo_root" cat-file -e "${accepted_stage5g_a_ref}^{commit}" 2>/dev/null; then
  echo "stage5g-a-snapshot-gate: FAIL: accepted Stage 5G-a commit unavailable" >&2
  exit 1
fi
if ! git -C "$repo_root" merge-base --is-ancestor "$accepted_stage5g_a_ref" HEAD; then
  echo "stage5g-a-snapshot-gate: FAIL: accepted Stage 5G-a is not an ancestor" >&2
  exit 1
fi

snapshot_parent="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-a-snapshot.XXXXXX")"
snapshot_root="$snapshot_parent/repository"
cleanup() {
  rm -rf "$snapshot_parent"
}
trap cleanup EXIT HUP INT TERM

git clone --quiet --shared --no-checkout "$repo_root" "$snapshot_root"
git -C "$snapshot_root" checkout --quiet --detach "$accepted_stage5g_a_ref"
if [[ "$(git -C "$snapshot_root" rev-parse HEAD)" != "$accepted_stage5g_a_ref" ]]; then
  echo "stage5g-a-snapshot-gate: FAIL: detached checkout drift" >&2
  exit 1
fi

entry_output="$({
  cd "$snapshot_root"
  "$python_bin" scripts/stage5g_entry_plan_check.py
})"
negative_output="$({
  cd "$snapshot_root"
  "$python_bin" scripts/stage5g_entry_plan_negative_harness.py
})"
printf '%s\n' "$entry_output"
printf '%s\n' "$negative_output"

if [[ "$entry_output" != *"stage5g-entry-plan-check: ok cases=54"* ]]; then
  echo "stage5g-a-snapshot-gate: FAIL: exact 54-case marker missing" >&2
  exit 1
fi
negative_pass_count="$(printf '%s\n' "$negative_output" | grep -c '^PASS ')"
if [[ "$negative_pass_count" != "30" ]] \
  || [[ "$negative_output" != *"stage5g-entry-plan-negative-harness: ok cases=30"* ]]; then
  echo "stage5g-a-snapshot-gate: FAIL: exact 30/30 marker missing" >&2
  exit 1
fi

echo "stage5g-a-snapshot-gate: PASS source_ref=$accepted_stage5g_a_ref entry_cases=54 negative_cases=30/30"
