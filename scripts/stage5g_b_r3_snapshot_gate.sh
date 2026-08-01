#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ref="92f57c7831d8a15fb2e37668d3b07f1ccea03af7"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-b-r3-snapshot.XXXXXX")"
trap 'chmod -R u+w "$tmp" 2>/dev/null || true; rm -rf "$tmp"' EXIT

git -C "$root" clone --quiet --shared --no-checkout "$root" "$tmp/repo"
git -C "$tmp/repo" checkout --quiet --detach "$ref"
bash "$tmp/repo/scripts/stage5g_a_snapshot_gate.sh"
bash "$tmp/repo/scripts/stage5g_b_r1_snapshot_gate.sh"
bash "$tmp/repo/scripts/stage5g_b_r2_snapshot_gate.sh"
python3 "$tmp/repo/scripts/stage5g_b_r3_check.py" --root "$tmp/repo"
python3 "$tmp/repo/scripts/stage5g_b_r3_negative_harness.py"

echo "stage5g-b-r3-snapshot-gate: ok ref=$ref r3_negative=6/6 inherited=green"
