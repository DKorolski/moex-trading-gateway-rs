#!/usr/bin/env bash
set -euo pipefail

git diff --check
python3 scripts/stage8b_p1d3_design_check.py
python3 scripts/stage8b_p1d3_design_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1d3_design_check.py \
  scripts/stage8b_p1d3_design_negative_harness.py \
  scripts/make_stage8b_p1d3_design_handoff.py \
  scripts/stage8b_p1d3_design_handoff_safety_check.py
python3 -m json.tool \
  docs/stage-8/stage8b-p1d3-working-limit-cancel-evidence.json \
  >/dev/null
python3 - <<'PY'
import csv
from pathlib import Path

path = Path("docs/stage-8/stage8b-p1d3-working-limit-cancel-acceptance-matrix.csv")
with path.open(newline="", encoding="utf-8") as stream:
    rows = list(csv.DictReader(stream))
if len(rows) != 92:
    raise SystemExit(f"matrix row count drifted: {len(rows)}")
if [row["id"] for row in rows] != [f"P1D3D-{index:03d}" for index in range(1, 93)]:
    raise SystemExit("matrix ordering drifted")
print("PASS stage8b-p1d3-r1-design-matrix rows=92")
PY

echo "PASS stage8b-p1d3-r1-design-gate rows=92 negatives=58 shapes=8 design_only=true source=false p1d4=false db0=false finam=false live=false"
