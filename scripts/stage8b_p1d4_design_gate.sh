#!/usr/bin/env bash
set -euo pipefail

git diff --check
python3 scripts/stage8b_p1d4_design_check.py
python3 scripts/stage8b_p1d4_design_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1d4_design_check.py \
  scripts/stage8b_p1d4_design_negative_harness.py \
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
if len(rows) != 72:
    raise SystemExit(f"matrix row count drifted: {len(rows)}")
if [row["id"] for row in rows] != [f"P1D4D-{index:03d}" for index in range(1, 73)]:
    raise SystemExit("matrix ordering drifted")
print("PASS stage8b-p1d4-design-matrix rows=72")

cell_path = Path("docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v1.csv")
cell_bytes = cell_path.read_bytes()
with cell_path.open(newline="", encoding="utf-8") as stream:
    cells = list(csv.DictReader(stream))
if len(cells) != 80:
    raise SystemExit(f"scenario/frontier cell count drifted: {len(cells)}")
if hashlib.sha256(cell_bytes).hexdigest() != "d1d765f1fa6db1dc948725273f58938c1d0cabd614d26076bf3ac2ddd1ad36ed":
    raise SystemExit("scenario/frontier matrix digest drifted")
print("PASS stage8b-p1d4-r1-cell-matrix cells=80 scenarios=11 frontiers=20")
PY

echo "PASS stage8b-p1d4-r1-design-gate rows=72 cells=80 negatives=60 frontiers=20 design_only=true implementation=false db0=false finam=false live=false"
