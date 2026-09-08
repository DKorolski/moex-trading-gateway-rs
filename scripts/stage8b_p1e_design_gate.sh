#!/usr/bin/env bash
set -euo pipefail

git diff --check
python3 scripts/stage8b_p1e_design_check.py
python3 scripts/stage8b_p1e_design_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1e_design_check.py \
  scripts/stage8b_p1e_design_negative_harness.py \
  scripts/make_stage8b_p1e_design_handoff.py \
  scripts/stage8b_p1e_design_handoff_safety_check.py
python3 -m json.tool docs/stage-8/stage8b-p1e-deployable-supervisor-design-evidence.json >/dev/null
python3 - <<'PY'
import csv
import hashlib
from pathlib import Path

matrix = Path("docs/stage-8/stage8b-p1e-deployable-supervisor-acceptance-matrix.csv")
with matrix.open(newline="", encoding="utf-8") as stream:
    rows = list(csv.DictReader(stream))
if len(rows) != 48:
    raise SystemExit(f"acceptance row count drifted: {len(rows)}")
if [row["id"] for row in rows] != [f"P1E-{index:03d}" for index in range(1, 49)]:
    raise SystemExit("acceptance ordering drifted")

expected = {
    "deploy/paper-shadow/moex-finam-paper-runtime.service": "8f8f2854191887a75317869c8e6ff3c8edd4197c1ae594fa93c0b56fc35585fc",
    "deploy/paper-shadow/moex-finam-paper-ws.service": "e61591a064838725a2fc15ee089fb862bd62df97f8995549773f9f3ee65cf1b9",
}
for name, digest in expected.items():
    actual = hashlib.sha256(Path(name).read_bytes()).hexdigest()
    if actual != digest:
        raise SystemExit(f"accepted P0 unit drifted: {name}")
print("PASS stage8b-p1e-design-matrix rows=48")
print("PASS stage8b-p1e-p0-units-unchanged units=2")
PY

echo "PASS stage8b-p1e-design-gate rows=48 negatives=36 design_only=true implementation=false activation=false db0=false finam=false live=false"
