#!/usr/bin/env bash
set -euo pipefail

git diff --check HEAD^ HEAD
python3 scripts/stage8b_p1d4_r6_design_check.py
python3 scripts/stage8b_p1d4_r6_design_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1d4_r6_design_check.py \
  scripts/stage8b_p1d4_r6_design_negative_harness.py \
  scripts/make_stage8b_p1d4_r6_design_handoff.py \
  scripts/stage8b_p1d4_r6_design_handoff_safety_check.py
python3 -m json.tool \
  docs/stage-8/stage8b-p1d4-crash-replay-evidence-r6.json >/dev/null
python3 -m json.tool \
  docs/stage-8/stage8b-p1d4-source-shape-r6.json >/dev/null
python3 - <<'PY'
import csv
import hashlib
from pathlib import Path

checks = (
    ("docs/stage-8/stage8b-p1d4-crash-replay-acceptance-matrix.csv", 88, "e2a376fda504a691b3dffa5160542c5f184bf59ff9f224ae4159b5bbaf8c06de"),
    ("docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v5.csv", 92, "f4125113d4f7f3ceafd849c1d32f1097c313e15eccbb353c8bd89a19434edac6"),
    ("docs/stage-8/stage8b-p1d4-generated-market-crash-submatrix-v2.csv", 13, "e6a284372cc99fab3bfeadd9afa0d8e1ddec0070966f79505a5cb09dbb627561"),
    ("docs/stage-8/stage8b-p1d4-r6-acceptance-amendment.csv", 32, "ca381b8e6891840303d9c602ca27e379a072a3cf344526f14ee1ce170e5f7f53"),
)
for name, count, digest in checks:
    path = Path(name)
    with path.open(newline="", encoding="utf-8") as stream:
        rows = list(csv.DictReader(stream))
    if len(rows) != count:
        raise SystemExit(f"{name}: row count {len(rows)} != {count}")
    if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
        raise SystemExit(f"{name}: digest drifted")
print("PASS stage8b-p1d4-r6-matrices general=88 base=92 generated_market=13 amendments=32 active=105")
PY

echo "PASS stage8b-p1d4-r6-design-gate active_cells=105 targeted_negatives=48 inherited_negatives=128 source_shape=exact option=A source_m10=1 xack_last=true design_only=true implementation=false db0=false finam=false live=false"
