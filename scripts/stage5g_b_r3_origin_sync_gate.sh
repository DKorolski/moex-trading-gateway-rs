#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
branch="$(git -C "$root" branch --show-current)"
head_ref="$(git -C "$root" rev-parse HEAD)"
origin_ref="$(git -C "$root" rev-parse origin/stage5g-lifecycle)"

test "$branch" = "stage5g-lifecycle"
test "$head_ref" = "$origin_ref"
git -C "$root" cat-file -e "$origin_ref^{commit}"

echo "stage5g-b-r3-origin-sync-gate: PASS branch=$branch source_ref=$head_ref origin_ref=$origin_ref"
