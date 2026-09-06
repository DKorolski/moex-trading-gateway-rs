#!/usr/bin/env bash
set -euo pipefail

git diff --check HEAD^ HEAD
python3 scripts/stage8b_p1d4_r7_design_check.py
python3 scripts/stage8b_p1d4_r7_design_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1d4_r7_design_check.py \
  scripts/stage8b_p1d4_r7_design_negative_harness.py \
  scripts/make_stage8b_p1d4_r7_design_handoff.py \
  scripts/stage8b_p1d4_r7_design_handoff_safety_check.py
python3 -m json.tool \
  docs/stage-8/stage8b-p1d4-crash-replay-evidence-r7.json >/dev/null
python3 -m json.tool \
  docs/stage-8/stage8b-p1d4-source-shape-r7.json >/dev/null
python3 -m json.tool \
  docs/stage-8/fixtures/stage8b-p1d4-command-publication-binding-v1.json >/dev/null
python3 - <<'PY'
import csv
import hashlib
from pathlib import Path

checks = (
    ("docs/stage-8/stage8b-p1d4-crash-replay-acceptance-matrix.csv", 88, "e2a376fda504a691b3dffa5160542c5f184bf59ff9f224ae4159b5bbaf8c06de"),
    ("docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v5.csv", 92, "f4125113d4f7f3ceafd849c1d32f1097c313e15eccbb353c8bd89a19434edac6"),
    ("docs/stage-8/stage8b-p1d4-generated-market-crash-submatrix-v3.csv", 13, "99b30f93c2c7b3c281f6f96c7eacab677e45b32bd89ec81d1847259cb39d514e"),
    ("docs/stage-8/stage8b-p1d4-r7-acceptance-amendment.csv", 40, "6cd2525bd748b913e7a90e1d0448a4f9f86141c3438031832edbfd66e6d44ebf"),
)
for name, count, digest in checks:
    path = Path(name)
    with path.open(newline="", encoding="utf-8") as stream:
        rows = list(csv.DictReader(stream))
    if len(rows) != count:
        raise SystemExit(f"{name}: row count {len(rows)} != {count}")
    if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
        raise SystemExit(f"{name}: digest drifted")
print("PASS stage8b-p1d4-r7-matrices general=88 base=92 generated_market=13 amendments=40 active=105")
PY

echo "PASS stage8b-p1d4-r7-design-gate active_cells=105 targeted_negatives=60 inherited_negatives=128 reservation=precommitted routing=package_aware fixture=exact xack_last=true design_only=true implementation=false db0=false finam=false live=false"
