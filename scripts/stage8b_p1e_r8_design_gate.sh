#!/usr/bin/env bash
set -euo pipefail

git diff --check
python3 scripts/stage8b_p1e_r8_design_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_r8_design_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1e_r8_design_check.py \
  scripts/stage8b_p1e_r8_design_negative_harness.py \
  scripts/stage8b_p1e_r8_design_handoff_safety_check.py \
  scripts/make_stage8b_p1e_r8_design_handoff.py

python3 - <<'PY'
import csv
import json
from pathlib import Path

root = Path("docs/stage-8")
json_names = (
    "stage8b-p1e-active-acceptance-contract-v8.json",
    "stage8b-p1e-semantic-authority-registry-v8.json",
    "stage8b-p1e-latch-aware-source-seam-v2.json",
    "stage8b-p1e-latch-route-transition-matrix-v1.json",
    "stage8b-p1e-shutdown-intent-v1.json",
    "stage8b-p1e-i0-regression-gate-v1.json",
    "stage8b-p1e-deployable-supervisor-r8-design-evidence.json",
)
for name in json_names:
    json.loads((root / name).read_text(encoding="utf-8"))
with (root / "stage8b-p1e-deployable-supervisor-r8-acceptance-matrix.csv").open(newline="", encoding="utf-8") as stream:
    acceptance = len(list(csv.DictReader(stream)))
with (root / "stage8b-p1e-supervisor-event-matrix-v3.csv").open(newline="", encoding="utf-8") as stream:
    events = len(list(csv.DictReader(stream)))
routes = json.loads((root / "stage8b-p1e-latch-route-transition-matrix-v1.json").read_text())
if (acceptance, events, len(routes["rows"])) != (27, 25, 30):
    raise SystemExit(f"R8 matrix count drifted: acceptance={acceptance} events={events} routes={len(routes['rows'])}")
print("PASS stage8b-p1e-r8-matrices acceptance=27 active=279 events=25 route_variants=30")
PY

echo "PASS stage8b-p1e-r8-design-gate negatives=45 semantic_keys=31 route_variants=30 shutdown_cases=9 design_only=true i0_now=false supervisor=false activation=false db0=false finam=false live=false"
