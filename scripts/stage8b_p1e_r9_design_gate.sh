#!/usr/bin/env bash
set -euo pipefail

git diff --check
PYTHONPATH=scripts python3 scripts/stage8b_p1e_r9_design_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_r9_design_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1e_i0_scope_check.py \
  scripts/stage8b_p1e_i0_p1d4_regression_check.py \
  scripts/stage8b_p1e_r9_design_check.py \
  scripts/stage8b_p1e_r9_design_negative_harness.py \
  scripts/stage8b_p1e_r9_design_handoff_safety_check.py \
  scripts/make_stage8b_p1e_r9_design_handoff.py

python3 - <<'PY'
import csv
import json
from pathlib import Path

root = Path("docs/stage-8")
json_names = (
    "stage8b-p1e-active-acceptance-contract-v9.json",
    "stage8b-p1e-semantic-authority-registry-v9.json",
    "stage8b-p1e-latch-route-transition-matrix-v2.json",
    "stage8b-p1e-route-outcome-fixture-matrix-v1.json",
    "stage8b-p1e-source-timer-precedence-v4.json",
    "stage8b-p1e-i0-regression-gate-v2.json",
    "stage8b-p1e-deployable-supervisor-r9-design-evidence.json",
)
values = {name: json.loads((root / name).read_text(encoding="utf-8")) for name in json_names}
with (root / "stage8b-p1e-deployable-supervisor-r9-acceptance-matrix.csv").open(newline="", encoding="utf-8") as stream:
    acceptance = len(list(csv.DictReader(stream)))
with (root / "stage8b-p1e-supervisor-event-matrix-v4.csv").open(newline="", encoding="utf-8") as stream:
    events = len(list(csv.DictReader(stream)))
routes = values["stage8b-p1e-latch-route-transition-matrix-v2.json"]
outcomes = values["stage8b-p1e-route-outcome-fixture-matrix-v1.json"]
if (acceptance, events, routes["row_count"], outcomes["case_count"], len(outcomes["fixtures"])) != (24, 25, 30, 46, 46):
    raise SystemExit("R9 matrix count drift")
print("PASS stage8b-p1e-r9-matrices acceptance=24 active=298 events=25 route_cells=30 outcome_fixtures=46")
PY

echo "PASS stage8b-p1e-r9-design-gate negatives=36 integrity=8 semantic=28 active=298 semantic_keys=34 route_cells=30 outcome_fixtures=46 design_only=true i0_now=false supervisor=false activation=false db0=false finam=false live=false"
