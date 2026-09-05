#!/usr/bin/env bash
set -euo pipefail

git diff --check
python3 scripts/stage8b_p1d4_r2_design_check.py
python3 scripts/stage8b_p1d4_r2_design_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1d4_r2_design_check.py \
  scripts/stage8b_p1d4_r2_design_negative_harness.py \
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
if len(rows) != 80:
    raise SystemExit(f"matrix row count drifted: {len(rows)}")
if [row["id"] for row in rows] != [f"P1D4D-{index:03d}" for index in range(1, 81)]:
    raise SystemExit("matrix ordering drifted")
if hashlib.sha256(path.read_bytes()).hexdigest() != "57f9e83c9a4d8c6b56cb39792c7e08f63e2717c261c508fdd443317b78aac4c0":
    raise SystemExit("general matrix digest drifted")
print("PASS stage8b-p1d4-r2-design-matrix rows=80")

cell_path = Path("docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v2.csv")
cell_bytes = cell_path.read_bytes()
with cell_path.open(newline="", encoding="utf-8") as stream:
    cells = list(csv.DictReader(stream))
if len(cells) != 92:
    raise SystemExit(f"scenario/frontier cell count drifted: {len(cells)}")
if hashlib.sha256(cell_bytes).hexdigest() != "b54d8d26e5ebb12389946c905f37a029beb85d1005c7ef95edf6a47596bd725a":
    raise SystemExit("scenario/frontier matrix digest drifted")
print("PASS stage8b-p1d4-r2-cell-matrix cells=92 scenarios=11 frontiers=21")
PY

echo "PASS stage8b-p1d4-r2-design-gate rows=80 cells=92 negatives=104 frontiers=21 design_only=true implementation=false db0=false finam=false live=false"
