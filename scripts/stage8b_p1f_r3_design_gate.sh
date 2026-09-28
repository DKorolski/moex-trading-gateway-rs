#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

scripts/stage8b_p1f_r3_design_check.py
scripts/stage8b_p1f_r3_design_negative_harness.py

R2_ZIP="reports/handoff/moex-trading-project-eeac091-stage8b-p1f-r2-design-correction.zip"
R2_SHA="d759b199dbdd944838de44256ecccd04d75a91d9313c53cff179636e25e8f4b3"
[[ -f "$R2_ZIP" ]]
[[ "$(shasum -a 256 "$R2_ZIP" | awk '{print $1}')" == "$R2_SHA" ]]
scripts/stage8b_p1f_r2_design_handoff_safety_check.py "$R2_ZIP"

REVIEW="/Users/denisq/Downloads/FINAM_P1F_R2_DESIGN_REVIEW_eeac091_2026-09-25.md"
[[ -f "$REVIEW" ]]
[[ "$(shasum -a 256 "$REVIEW" | awk '{print $1}')" == "cdb156224dc76349b20a710d043bbf069c5a38e4231546e831f723cf105a2fb6" ]]

if git diff --name-only eeac091635f102fa5b7dc9db3564c214e2efefe0 -- \
    Cargo.toml Cargo.lock crates deploy config .github/workflows | grep -q .; then
  echo "stage8b-p1f-r3-design-gate: FAIL production surface changed" >&2
  exit 1
fi

echo "PASS stage8b-p1f-r3-design-gate rows=72 models=39 artifacts=9 scripts=8 operations=10 traces=2 routes=6 roles=8 negatives=48 remote_mutation=false activation=false"
