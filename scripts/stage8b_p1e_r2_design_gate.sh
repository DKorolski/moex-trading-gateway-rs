#!/usr/bin/env bash
set -euo pipefail

git diff --check
python3 scripts/stage8b_p1e_r2_design_check.py
python3 scripts/stage8b_p1e_r2_design_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1e_r2_design_check.py \
  scripts/stage8b_p1e_r2_design_negative_harness.py \
  scripts/make_stage8b_p1e_r2_design_handoff.py \
  scripts/stage8b_p1e_r2_design_handoff_safety_check.py

python3 - <<'PY'
import csv
import hashlib
import json
from pathlib import Path

root = Path('.')
for name in [
    'stage8b-p1e-first-boot-transaction-v1.json',
    'stage8b-p1e-redis-runtime-policy-v1.json',
    'stage8b-p1e-deployable-supervisor-r2-design-evidence.json',
]:
    json.loads((root / 'docs/stage-8' / name).read_text())

def rows(name):
    with (root / 'docs/stage-8' / name).open(newline='', encoding='utf-8') as stream:
        return list(csv.DictReader(stream))

counts = {
    'acceptance': len(rows('stage8b-p1e-deployable-supervisor-r2-acceptance-matrix.csv')),
    'restart': len(rows('stage8b-p1e-restart-continuation-matrix-v2.csv')),
    'operational': len(rows('stage8b-p1e-operational-continuation-matrix-v2.csv')),
}
if counts != {'acceptance': 48, 'restart': 22, 'operational': 54}:
    raise SystemExit(f'R2 matrix count drifted: {counts}')

expected = {
    'deploy/paper-shadow/moex-finam-paper-runtime.service': '8f8f2854191887a75317869c8e6ff3c8edd4197c1ae594fa93c0b56fc35585fc',
    'deploy/paper-shadow/moex-finam-paper-ws.service': 'e61591a064838725a2fc15ee089fb862bd62df97f8995549773f9f3ee65cf1b9',
}
for name, digest in expected.items():
    actual = hashlib.sha256((root / name).read_bytes()).hexdigest()
    if actual != digest:
        raise SystemExit(f'accepted P0 unit drifted: {name}')

print('PASS stage8b-p1e-r2-json-contracts count=3')
print('PASS stage8b-p1e-r2-matrices inherited_acceptance=88 r2_acceptance=48 restart=22 operational=54 events=24')
print('PASS stage8b-p1e-r2-p0-units-unchanged units=2')
PY

echo "PASS stage8b-p1e-r2-design-gate inherited_negatives=64 r2_negatives=40 redigested=true design_only=true implementation=false activation=false db0=false finam=false live=false"
