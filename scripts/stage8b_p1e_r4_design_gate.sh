#!/usr/bin/env bash
set -euo pipefail

git diff --check
python3 scripts/stage8b_p1e_r4_design_check.py
python3 scripts/stage8b_p1e_r4_design_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1e_r4_design_check.py \
  scripts/stage8b_p1e_r4_design_negative_harness.py \
  scripts/stage8b_p1e_r4_design_handoff_safety_check.py \
  scripts/make_stage8b_p1e_r4_design_handoff.py

python3 - <<'PY'
import csv
import json
from pathlib import Path

root = Path("docs/stage-8")
json_names = [
    "stage8b-p1e-active-acceptance-contract-v4.json",
    "stage8b-p1e-semantic-authority-registry-v4.json",
    "stage8b-p1e-deployment-identity-v1.json",
    "stage8b-p1e-first-boot-transaction-v3.json",
    "stage8b-p1e-first-boot-receipt-v2.json",
    "stage8b-p1e-derived-digests-v1.json",
    "stage8b-p1e-derived-digests-v1-golden.json",
    "stage8b-p1e-acquisition-model-v2.json",
    "stage8b-p1e-source-timer-precedence-v1.json",
    "stage8b-p1e-authenticated-restart-package-v2.json",
    "stage8b-p1e-deployable-supervisor-r4-design-evidence.json",
]
for name in json_names:
    json.loads((root / name).read_text(encoding="utf-8"))

def count(name: str) -> int:
    with (root / name).open(newline="", encoding="utf-8") as stream:
        return len(list(csv.DictReader(stream)))

counts = {
    "r1": count("stage8b-p1e-deployable-supervisor-r1-acceptance-matrix.csv"),
    "r2": count("stage8b-p1e-deployable-supervisor-r2-acceptance-matrix.csv"),
    "r3": count("stage8b-p1e-deployable-supervisor-r3-acceptance-matrix.csv"),
    "r4": count("stage8b-p1e-deployable-supervisor-r4-acceptance-matrix.csv"),
    "outer": count("stage8b-p1e-restart-continuation-matrix-v2.csv"),
    "operational": count("stage8b-p1e-operational-pretransition-matrix-v4.csv"),
}
expected = {"r1": 88, "r2": 48, "r3": 43, "r4": 56, "outer": 22, "operational": 52}
if counts != expected:
    raise SystemExit(f"matrix count drifted: {counts}")
print("PASS stage8b-p1e-r4-matrices acceptance=182 restart=22 operational=52")
PY

echo "PASS stage8b-p1e-r4-design-gate negatives=48 semantic_keys=14 first_boot=10+4 acquisition_model=B design_only=true source=false activation=false db0=false finam=false live=false"
