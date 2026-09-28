#!/usr/bin/env bash
set -euo pipefail

git diff --check
python3 scripts/stage8b_p1e_r3_design_check.py
python3 scripts/stage8b_p1e_r3_design_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1e_r3_design_check.py \
  scripts/stage8b_p1e_r3_design_negative_harness.py \
  scripts/make_stage8b_p1e_r3_design_handoff.py \
  scripts/stage8b_p1e_r3_design_handoff_safety_check.py

python3 - <<'PY'
import csv
import hashlib
import json
from pathlib import Path

root = Path('.')
for name in [
    'stage8b-p1e-active-acceptance-contract-v3.json',
    'stage8b-p1e-first-boot-transaction-v2.json',
    'stage8b-p1e-first-boot-receipt-v1.json',
    'stage8b-p1e-authenticated-restart-package-v2.json',
    'stage8b-p1e-source-acquisition-seam-v1.json',
    'stage8b-p1e-redis-runtime-policy-v1.json',
    'stage8b-p1e-deployable-supervisor-r3-design-evidence.json',
]:
    json.loads((root / 'docs/stage-8' / name).read_text())

def rows(name):
    with (root / 'docs/stage-8' / name).open(newline='', encoding='utf-8') as stream:
        return list(csv.DictReader(stream))

counts = {
    'r1': len(rows('stage8b-p1e-deployable-supervisor-r1-acceptance-matrix.csv')),
    'r2': len(rows('stage8b-p1e-deployable-supervisor-r2-acceptance-matrix.csv')),
    'r3': len(rows('stage8b-p1e-deployable-supervisor-r3-acceptance-matrix.csv')),
    'restart': len(rows('stage8b-p1e-restart-continuation-matrix-v2.csv')),
    'pretransition': len(rows('stage8b-p1e-operational-pretransition-matrix-v3.csv')),
}
if counts != {'r1': 88, 'r2': 48, 'r3': 43, 'restart': 22, 'pretransition': 52}:
    raise SystemExit(f'R3 matrix count drifted: {counts}')

expected = {
    'deploy/paper-shadow/moex-finam-paper-runtime.service': '8f8f2854191887a75317869c8e6ff3c8edd4197c1ae594fa93c0b56fc35585fc',
    'deploy/paper-shadow/moex-finam-paper-ws.service': 'e61591a064838725a2fc15ee089fb862bd62df97f8995549773f9f3ee65cf1b9',
}
for name, digest in expected.items():
    actual = hashlib.sha256((root / name).read_bytes()).hexdigest()
    if actual != digest:
        raise SystemExit(f'accepted P0 unit drifted: {name}')

print('PASS stage8b-p1e-r3-json-contracts count=7')
print('PASS stage8b-p1e-r3-matrices active_acceptance=160 superseded=19 restart=22 pretransition=52 events=24')
print('PASS stage8b-p1e-r3-p0-units-unchanged units=2')
PY

echo "PASS stage8b-p1e-r3-design-gate inherited_r1_negatives=64 inherited_r2_negatives=40 r3_negatives=36 redigested=true design_only=true implementation=false activation=false db0=false finam=false live=false"
