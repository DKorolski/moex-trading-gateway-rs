#!/usr/bin/env bash
set -euo pipefail

git diff --check
python3 scripts/stage8b_p1d4_r3_design_check.py
python3 scripts/stage8b_p1d4_r3_design_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1d4_r3_design_check.py \
  scripts/stage8b_p1d4_r3_design_negative_harness.py \
  scripts/make_stage8b_p1d4_design_handoff.py \
  scripts/stage8b_p1d4_design_handoff_safety_check.py
python3 -m json.tool docs/stage-8/stage8b-p1d4-crash-replay-evidence.json >/dev/null
python3 - <<'PY'
import csv
import hashlib
from pathlib import Path

path = Path("docs/stage-8/stage8b-p1d4-crash-replay-acceptance-matrix.csv")
with path.open(newline="", encoding="utf-8") as stream:
    rows = list(csv.DictReader(stream))
if len(rows) != 88:
    raise SystemExit(f"matrix row count drifted: {len(rows)}")
if [row["id"] for row in rows] != [f"P1D4D-{index:03d}" for index in range(1, 89)]:
    raise SystemExit("matrix ordering drifted")
if hashlib.sha256(path.read_bytes()).hexdigest() != "e2a376fda504a691b3dffa5160542c5f184bf59ff9f224ae4159b5bbaf8c06de":
    raise SystemExit("general matrix digest drifted")
print("PASS stage8b-p1d4-r3-design-matrix rows=88")

cell_path = Path("docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v3.csv")
cell_bytes = cell_path.read_bytes()
with cell_path.open(newline="", encoding="utf-8") as stream:
    cells = list(csv.DictReader(stream))
if len(cells) != 92:
    raise SystemExit(f"scenario/frontier cell count drifted: {len(cells)}")
if hashlib.sha256(cell_bytes).hexdigest() != "8c3122af016e860e3fa54a6f4143c50b9258c2a5a4bd2d15f9847a118a8582cc":
    raise SystemExit("scenario/frontier matrix digest drifted")
print("PASS stage8b-p1d4-r3-cell-matrix cells=92 scenarios=11 frontiers=21")
PY

echo "PASS stage8b-p1d4-r3-design-gate rows=88 cells=92 negatives=128 frontiers=21 design_only=true implementation=false db0=false finam=false live=false"
