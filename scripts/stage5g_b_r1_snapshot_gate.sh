#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ref="00d158978904c177828ff2a330b1f3c1bfb4bb10"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-b-r1-snapshot.XXXXXX")"
trap 'chmod -R u+w "$tmp" 2>/dev/null || true; rm -rf "$tmp"' EXIT

git -C "$root" clone --quiet --shared --no-checkout "$root" "$tmp/repo"
git -C "$tmp/repo" checkout --quiet --detach "$ref"
python3 "$tmp/repo/scripts/stage5g_b_mock_ack_check.py" --root "$tmp/repo"
python3 "$tmp/repo/scripts/stage5g_b_mock_ack_negative_harness.py"
python3 "$tmp/repo/scripts/stage5g_b_r1_check.py" --root "$tmp/repo"
python3 "$tmp/repo/scripts/stage5g_b_r1_negative_harness.py"

echo "stage5g-b-r1-snapshot-gate: ok ref=$ref base_negative=15/15 r1_negative=18/18"
