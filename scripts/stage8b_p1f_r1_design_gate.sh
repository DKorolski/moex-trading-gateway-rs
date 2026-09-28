#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

scripts/stage8b_p1f_r1_design_check.py
scripts/stage8b_p1f_r1_design_negative_harness.py

R0_ZIP="reports/handoff/moex-trading-project-58bb4ca-stage8b-p1f-r0-design.zip"
R0_SHA="ae57fa38fb2cab9ae8d8943351624df8e4a58a72ff533b3ca8bde2156005549a"
[[ -f "$R0_ZIP" ]]
[[ "$(shasum -a 256 "$R0_ZIP" | awk '{print $1}')" == "$R0_SHA" ]]
scripts/stage8b_p1f_design_handoff_safety_check.py "$R0_ZIP"

REVIEW="/Users/denisq/Downloads/FINAM_P1F_R0_DESIGN_REVIEW_58bb4ca_2026-09-25.md"
[[ -f "$REVIEW" ]]
[[ "$(shasum -a 256 "$REVIEW" | awk '{print $1}')" == "ea5b184ad7c412e63229c8b98bacda895da151c09fc666057dc512968cc24ba5" ]]

if git diff --name-only 58bb4cafd3eb80f43c8d0bfd182f7be18ea92d00 -- \
    Cargo.toml Cargo.lock crates deploy config .github/workflows | grep -q .; then
  echo "stage8b-p1f-r1-design-gate: FAIL production surface changed" >&2
  exit 1
fi

echo "PASS stage8b-p1f-r1-design-gate rows=64 models=20 artifacts=8 roles=8 negatives=44 remote_mutation=false activation=false"
