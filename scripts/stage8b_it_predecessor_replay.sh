#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
accepted_i_ref="0af222f252cdc2b4c763c9e04935a5cb5f0c6d65"
work="$(mktemp -d "${TMPDIR:-/tmp}/stage8b-it-i-r3-replay.XXXXXX")"
trap 'rm -rf "$work"' EXIT

git -C "$repo_root" cat-file -e "${accepted_i_ref}^{commit}"
git clone --quiet --no-hardlinks --shared "$repo_root" "$work/repo"
git -C "$work/repo" checkout --quiet --detach "$accepted_i_ref"
test "$(git -C "$work/repo" rev-parse HEAD)" = "$accepted_i_ref"
(
  cd "$work/repo"
  python3 scripts/stage8b_i_check.py
)

echo "stage8b-it-predecessor-replay: PASS source_ref=$accepted_i_ref stage8b_i_r3=true reopened=false"
