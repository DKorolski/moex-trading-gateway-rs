#!/usr/bin/env bash
set -euo pipefail

# The B3F provenance harness seals the exact B3F change set and must never be
# run against a later Stage 5F checkout. This wrapper is the sole supported
# runner for that inherited harness in both CI and handoff packaging.
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
accepted_b3f_ref="e14654f7129aa61011931306140a3bfefe2fcfbc"
expected_pass_cases=580
snapshot_root=""
output_log=""

cleanup() {
  local status=$?
  if [[ -n "$snapshot_root" ]]; then
    rm -rf "$snapshot_root"
  fi
  if [[ -n "$output_log" ]]; then
    rm -f "$output_log"
  fi
  exit "$status"
}
trap cleanup EXIT

if ! git -C "$repo_root" cat-file -e "${accepted_b3f_ref}^{commit}" 2>/dev/null; then
  echo "stage5f-b3f-snapshot-provenance-gate: FAIL: accepted B3F snapshot commit unavailable" >&2
  exit 1
fi

snapshot_root="$(mktemp -d "${TMPDIR:-/tmp}/stage5f-b3f-provenance.XXXXXX")"
output_log="$(mktemp "${TMPDIR:-/tmp}/stage5f-b3f-provenance-output.XXXXXX")"
git clone --quiet --shared --no-checkout "$repo_root" "$snapshot_root"
git -C "$snapshot_root" checkout --quiet --detach "$accepted_b3f_ref"

if [[ "$(git -C "$snapshot_root" rev-parse HEAD)" != "$accepted_b3f_ref" ]]; then
  echo "stage5f-b3f-snapshot-provenance-gate: FAIL: accepted B3F snapshot checkout drift" >&2
  exit 1
fi

(
  cd "$snapshot_root"
  python3 scripts/handoff_provenance_negative_harness.py
) | tee "$output_log"

pass_cases="$(grep -c '^PASS ' "$output_log" || true)"
if [[ "$pass_cases" -ne "$expected_pass_cases" ]]; then
  echo "stage5f-b3f-snapshot-provenance-gate: FAIL: expected ${expected_pass_cases} PASS cases, got ${pass_cases}" >&2
  exit 1
fi

echo "stage5f-b3f-snapshot-provenance-gate: ok tested_source_ref=${accepted_b3f_ref} cases=${pass_cases}"
