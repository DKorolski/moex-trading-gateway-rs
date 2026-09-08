#!/usr/bin/env bash
set -euo pipefail

git diff --check
python3 scripts/stage8b_p1e_r1_design_check.py
python3 scripts/stage8b_p1e_r1_design_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1e_r1_design_check.py \
  scripts/stage8b_p1e_r1_design_negative_harness.py \
  scripts/make_stage8b_p1e_r1_design_handoff.py \
  scripts/stage8b_p1e_r1_design_handoff_safety_check.py

python3 - <<'PY'
import csv
import hashlib
import json
from pathlib import Path

root = Path('.')
json_names = [
    'stage8b-p1e-runtime-profile-v1.json',
    'stage8b-p1e-first-boot-source-bundle-schema-v1.json',
    'stage8b-p1e-first-boot-source-plan-v1.json',
    'stage8b-p1e-redis-deployment-manifest-v1.json',
    'stage8b-p1e-telemetry-contract-v1.json',
    'stage8b-p1e-deployable-supervisor-r1-design-evidence.json',
]
for name in json_names:
    json.loads((root / 'docs/stage-8' / name).read_text())

def rows(name):
    with (root / 'docs/stage-8' / name).open(newline='', encoding='utf-8') as stream:
        return list(csv.DictReader(stream))

if len(rows('stage8b-p1e-deployable-supervisor-r1-acceptance-matrix.csv')) != 88:
    raise SystemExit('R1 acceptance matrix count drifted')
if len(rows('stage8b-p1e-restart-continuation-matrix-v1.csv')) != 22:
    raise SystemExit('restart matrix count drifted')
if len(rows('stage8b-p1e-supervisor-event-matrix-v1.csv')) != 24:
    raise SystemExit('event matrix count drifted')

expected = {
    'deploy/paper-shadow/moex-finam-paper-runtime.service': '8f8f2854191887a75317869c8e6ff3c8edd4197c1ae594fa93c0b56fc35585fc',
    'deploy/paper-shadow/moex-finam-paper-ws.service': 'e61591a064838725a2fc15ee089fb862bd62df97f8995549773f9f3ee65cf1b9',
}
for name, digest in expected.items():
    actual = hashlib.sha256((root / name).read_bytes()).hexdigest()
    if actual != digest:
        raise SystemExit(f'accepted P0 unit drifted: {name}')

print('PASS stage8b-p1e-r1-json-contracts count=6')
print('PASS stage8b-p1e-r1-matrices acceptance=88 restart=22 events=24')
print('PASS stage8b-p1e-r1-p0-units-unchanged units=2')
PY

echo "PASS stage8b-p1e-r1-design-gate negatives=64 design_only=true implementation=false activation=false db0=false finam=false live=false"
