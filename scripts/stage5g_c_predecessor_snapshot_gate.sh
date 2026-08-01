#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ref="dba5362444ec279391eed92ff28ebb4ceb729c09"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-c-predecessor.XXXXXX")"
trap 'chmod -R u+w "$tmp" 2>/dev/null || true; rm -rf "$tmp"' EXIT

git -C "$root" clone --quiet --shared --no-checkout "$root" "$tmp/repo"
git -C "$tmp/repo" checkout --quiet --detach "$ref"
bash "$tmp/repo/scripts/stage5g_c_gate.sh"

echo "stage5g-c-predecessor-snapshot-gate: ok ref=$ref"
