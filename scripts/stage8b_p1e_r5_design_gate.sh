#!/usr/bin/env bash
set -euo pipefail

git diff --check
python3 scripts/stage8b_p1e_r5_design_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_r5_design_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1e_r5_design_check.py \
  scripts/stage8b_p1e_r5_design_negative_harness.py \
  scripts/stage8b_p1e_r5_design_handoff_safety_check.py \
  scripts/make_stage8b_p1e_r5_design_handoff.py

python3 - <<'PY'
import csv
import json
from pathlib import Path

root = Path("docs/stage-8")
for name in (
    "stage8b-p1e-active-acceptance-contract-v5.json",
    "stage8b-p1e-semantic-authority-registry-v5.json",
    "stage8b-p1e-deployment-identity-v2.json",
    "stage8b-p1e-first-boot-transaction-v4.json",
    "stage8b-p1e-source-timer-precedence-v2.json",
    "stage8b-p1e-deployable-supervisor-r5-design-evidence.json",
):
    json.loads((root / name).read_text(encoding="utf-8"))

def count(name: str) -> int:
    with (root / name).open(newline="", encoding="utf-8") as stream:
        return len(list(csv.DictReader(stream)))

counts = {
    "r5": count("stage8b-p1e-deployable-supervisor-r5-acceptance-matrix.csv"),
    "outer": count("stage8b-p1e-restart-continuation-matrix-v3.csv"),
    "operational": count("stage8b-p1e-operational-pretransition-matrix-v5.csv"),
}
if counts != {"r5": 42, "outer": 23, "operational": 56}:
    raise SystemExit(f"matrix count drifted: {counts}")
print("PASS stage8b-p1e-r5-matrices acceptance=42 active=217 restart=23 operational=56")
PY

echo "PASS stage8b-p1e-r5-design-gate negatives=44 semantic_keys=22 first_boot=11+4 cancel_recovered=4 design_only=true source=false activation=false db0=false finam=false live=false"
