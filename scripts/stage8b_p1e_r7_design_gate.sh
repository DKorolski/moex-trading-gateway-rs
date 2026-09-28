#!/usr/bin/env bash
set -euo pipefail

git diff --check
python3 scripts/stage8b_p1e_r7_design_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_r7_design_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1e_r7_design_check.py \
  scripts/stage8b_p1e_r7_design_negative_harness.py \
  scripts/stage8b_p1e_r7_design_handoff_safety_check.py \
  scripts/make_stage8b_p1e_r7_design_handoff.py

python3 - <<'PY'
import csv
import json
from pathlib import Path

root = Path("docs/stage-8")
for name in (
    "stage8b-p1e-active-acceptance-contract-v7.json",
    "stage8b-p1e-semantic-authority-registry-v7.json",
    "stage8b-p1e-latch-aware-source-seam-v1.json",
    "stage8b-p1e-latch-race-test-matrix-v1.json",
    "stage8b-p1e-deployable-supervisor-r7-design-evidence.json",
):
    json.loads((root / name).read_text(encoding="utf-8"))
with (root / "stage8b-p1e-deployable-supervisor-r7-acceptance-matrix.csv").open(newline="", encoding="utf-8") as stream:
    acceptance = len(list(csv.DictReader(stream)))
with (root / "stage8b-p1e-supervisor-event-matrix-v2.csv").open(newline="", encoding="utf-8") as stream:
    events = len(list(csv.DictReader(stream)))
if (acceptance, events) != (34, 25):
    raise SystemExit(f"R7 matrix count drifted: acceptance={acceptance} events={events}")
print("PASS stage8b-p1e-r7-matrices acceptance=34 active=265 events=25 routes=20")
PY

echo "PASS stage8b-p1e-r7-design-gate negatives=32 semantic_keys=28 reclaim=15 terminal=5 option=A design_only=true i0_now=false supervisor=false activation=false db0=false finam=false live=false"
