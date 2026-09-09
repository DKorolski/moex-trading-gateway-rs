#!/usr/bin/env bash
set -euo pipefail

git diff --check
python3 scripts/stage8b_p1e_r6_design_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_r6_design_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1e_r6_design_check.py \
  scripts/stage8b_p1e_r6_design_negative_harness.py \
  scripts/stage8b_p1e_r6_design_handoff_safety_check.py \
  scripts/make_stage8b_p1e_r6_design_handoff.py

python3 - <<'PY'
import csv
import json
from pathlib import Path

root = Path("docs/stage-8")
for name in (
    "stage8b-p1e-active-acceptance-contract-v6.json",
    "stage8b-p1e-semantic-authority-registry-v6.json",
    "stage8b-p1e-acquisition-model-v3.json",
    "stage8b-p1e-operational-pretransition-overlay-v6.json",
    "stage8b-p1e-first-boot-transaction-v5.json",
    "stage8b-p1e-source-timer-precedence-v3.json",
    "stage8b-p1e-deployable-supervisor-r6-design-evidence.json",
):
    json.loads((root / name).read_text(encoding="utf-8"))
with (root / "stage8b-p1e-deployable-supervisor-r6-acceptance-matrix.csv").open(newline="", encoding="utf-8") as stream:
    count = len(list(csv.DictReader(stream)))
if count != 36:
    raise SystemExit(f"R6 matrix count drifted: {count}")
print("PASS stage8b-p1e-r6-matrices acceptance=36 active=242 operational=51")
PY

echo "PASS stage8b-p1e-r6-design-gate negatives=40 semantic_keys=26 reclaim_required=15 terminal=5 operational=51 design_only=true source=false activation=false db0=false finam=false live=false"
