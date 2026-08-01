#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ref="d03f6e5e88fb853290457d6d6dac08f21c2cf28b"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-b-r2-snapshot.XXXXXX")"
trap 'chmod -R u+w "$tmp" 2>/dev/null || true; rm -rf "$tmp"' EXIT

git -C "$root" clone --quiet --shared --no-checkout "$root" "$tmp/repo"
git -C "$tmp/repo" checkout --quiet --detach "$ref"
bash "$tmp/repo/scripts/stage5g_a_snapshot_gate.sh"
bash "$tmp/repo/scripts/stage5g_b_r1_snapshot_gate.sh"
python3 "$tmp/repo/scripts/stage5g_b_r2_check.py" --root "$tmp/repo"
python3 "$tmp/repo/scripts/stage5g_b_r2_negative_harness.py"

echo "stage5g-b-r2-snapshot-gate: ok ref=$ref r2_negative=12/12 inherited=green"
