#!/usr/bin/env bash
set -euo pipefail

git diff --check
python3 scripts/stage8b_p1d4_r4_design_check.py
python3 scripts/stage8b_p1d4_r4_design_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1d4_r4_design_check.py \
  scripts/stage8b_p1d4_r4_design_negative_harness.py \
  scripts/make_stage8b_p1d4_r4_design_handoff.py \
  scripts/stage8b_p1d4_r4_design_handoff_safety_check.py
python3 -m json.tool \
  docs/stage-8/stage8b-p1d4-crash-replay-evidence-r4.json >/dev/null
python3 - <<'PY'
import csv
import hashlib
from pathlib import Path

general = Path("docs/stage-8/stage8b-p1d4-crash-replay-acceptance-matrix.csv")
with general.open(newline="", encoding="utf-8") as stream:
    rows = list(csv.DictReader(stream))
if len(rows) != 88:
    raise SystemExit(f"general matrix row count drifted: {len(rows)}")
if hashlib.sha256(general.read_bytes()).hexdigest() != "e2a376fda504a691b3dffa5160542c5f184bf59ff9f224ae4159b5bbaf8c06de":
    raise SystemExit("general matrix digest drifted")
print("PASS stage8b-p1d4-r4-general-matrix rows=88 unchanged=true")

matrix = Path("docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v4.csv")
with matrix.open(newline="", encoding="utf-8") as stream:
    cells = list(csv.DictReader(stream))
if len(cells) != 92:
    raise SystemExit(f"proof-cell count drifted: {len(cells)}")
if hashlib.sha256(matrix.read_bytes()).hexdigest() != "74fc128b06d188942008449f05977d8eb46630c3ccfc364659a75d77d0e5810f":
    raise SystemExit("R4 registry digest drifted")
print("PASS stage8b-p1d4-r4-cell-matrix cells=92 corrections=4")
PY

echo "PASS stage8b-p1d4-r4-design-gate rows=88 cells=92 targeted_negatives=18 inherited_negatives=128 design_only=true implementation=false db0=false finam=false live=false"
