#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

scripts/stage8b_p1f_r2_design_check.py
scripts/stage8b_p1f_r2_design_negative_harness.py

R1_ZIP="reports/handoff/moex-trading-project-8feedfb-stage8b-p1f-r1-design-correction.zip"
R1_SHA="2fcf0abcf8f2a32671a6fcce2a973d91c1a791245e1a4be291ef0208cd654366"
[[ -f "$R1_ZIP" ]]
[[ "$(shasum -a 256 "$R1_ZIP" | awk '{print $1}')" == "$R1_SHA" ]]
scripts/stage8b_p1f_r1_design_handoff_safety_check.py "$R1_ZIP"

REVIEW="/Users/denisq/Downloads/FINAM_P1F_R1_DESIGN_REVIEW_8feedfb_2026-09-25.md"
[[ -f "$REVIEW" ]]
[[ "$(shasum -a 256 "$REVIEW" | awk '{print $1}')" == "163f57e6f524356b5ca57c645b0a28f5c334d369c83a31c75a7d2d910df2ad82" ]]

if git diff --name-only 8feedfb3e1d6e4d0f24148abdbbb25bf8d90b0ed -- \
    Cargo.toml Cargo.lock crates deploy config .github/workflows | grep -q .; then
  echo "stage8b-p1f-r2-design-gate: FAIL production surface changed" >&2
  exit 1
fi

echo "PASS stage8b-p1f-r2-design-gate rows=64 models=30 artifacts=9 scripts=8 operations=10 routes=6 roles=8 negatives=60 remote_mutation=false activation=false"
