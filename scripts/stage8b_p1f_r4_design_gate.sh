#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

scripts/stage8b_p1f_r4_design_check.py
scripts/stage8b_p1f_r4_design_negative_harness.py

R3_ZIP="reports/handoff/moex-trading-project-811ebe8-stage8b-p1f-r3-design-correction.zip"
R3_SHA="c5025e6408a82af3d12f4485f75d54009f5e6aa4ccc2d7fc9a3aa90c4beb2f40"
[[ -f "$R3_ZIP" ]]
[[ "$(shasum -a 256 "$R3_ZIP" | awk '{print $1}')" == "$R3_SHA" ]]
scripts/stage8b_p1f_r3_design_handoff_safety_check.py "$R3_ZIP"

REVIEW="/Users/denisq/Downloads/FINAM_P1F_R3_DESIGN_REVIEW_811ebe8_2026-09-25.md"
[[ -f "$REVIEW" ]]
[[ "$(shasum -a 256 "$REVIEW" | awk '{print $1}')" == "585872992198d80a0f05e76a791303c272b016903e9fbba1bcaf1132cf0365b9" ]]

if git diff --name-only 811ebe8ce22291311bd63fbf0cc5ff723261b758 -- \
    Cargo.toml Cargo.lock crates deploy config .github/workflows | grep -q .; then
  echo "stage8b-p1f-r4-design-gate: FAIL production surface changed" >&2
  exit 1
fi

echo "PASS stage8b-p1f-r4-design-gate rows=78 models=45 artifacts=9 scripts=8 operations=10 traces=2 routes=6 roles=8 negatives=43 remote_mutation=false activation=false"
